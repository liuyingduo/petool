import type { TimelineEvent } from '@/stores/chat'
import { normalizeToolName, renderToolLabel, truncateMiddle } from '@/utils/toolDisplay'

export type ToolStepStatus = 'running' | 'done' | 'error'

export interface CompactToolExecutionStep {
    id: string
    toolName: string
    title: string
    detail: string
    status: ToolStepStatus
    artifactPath: string
}

export interface CompactToolExecutionGroup {
    key: string
    firstEventId: string
    steps: CompactToolExecutionStep[]
}

export interface MonitorTodoItem {
    id: string
    label: string
    status: ToolStepStatus
}

export interface MonitorArtifactItem {
    id: string
    name: string
    path: string
    action: string
    status: ToolStepStatus
}

export interface TimelineTurnDisplay {
    turnId: string
    userText: string
    userCreatedAt: string
    assistantCreatedAt: string
    assistantEvents: TimelineEvent[]
}

export const TOOL_DETAIL_CACHE_MAX_ENTRIES = 800
export const timelineToolCompactDetailCache = new Map<string, string>()

export function setBoundedCacheValue<T>(cache: Map<string, T>, key: string, value: T, maxEntries: number) {
    cache.set(key, value)
    if (cache.size <= maxEntries) return
    const firstKey = cache.keys().next()
    if (!firstKey.done && firstKey.value !== undefined) {
        cache.delete(firstKey.value)
    }
}

export function clearTimelineToolCompactDetailCache() {
    timelineToolCompactDetailCache.clear()
}

export function getTimelinePayloadValue(event: TimelineEvent, key: string) {
    const payload = event.payload || {}
    return payload[key]
}

export function getTimelineText(event: TimelineEvent) {
    const value = getTimelinePayloadValue(event, 'text')
    return typeof value === 'string' ? value : String(value || '')
}

export function getTimelineReasoningText(event: TimelineEvent) {
    return getTimelineText(event)
}

export function getTimelineToolName(event: TimelineEvent) {
    const name = getTimelinePayloadValue(event, 'name')
    if (typeof name === 'string' && name.trim()) return renderToolLabel(name)
    return '工具执行'
}

export function getTimelineToolArguments(event: TimelineEvent) {
    const raw = getTimelinePayloadValue(event, 'argumentsChunk')
    if (typeof raw !== 'string') return ''
    return raw
}

export function getTimelineToolResult(event: TimelineEvent) {
    const error = getTimelinePayloadValue(event, 'error')
    if (typeof error === 'string' && error.trim()) {
        return JSON.stringify({ error }, null, 2)
    }

    const result = getTimelinePayloadValue(event, 'result')
    if (typeof result === 'string') return result
    if (result === null || result === undefined) return ''
    try {
        return JSON.stringify(result, null, 2)
    } catch {
        return String(result)
    }
}

export function getTimelineToolResultStatus(event: TimelineEvent): ToolStepStatus {
    const error = getTimelinePayloadValue(event, 'error')
    return typeof error === 'string' && error.trim() ? 'error' : 'done'
}

export function isTimelineToolEvent(event: TimelineEvent) {
    return event.event_type === 'assistant_tool_call' || event.event_type === 'assistant_tool_result'
}

export function getTimelineToolDisplayText(event: TimelineEvent) {
    if (event.event_type === 'assistant_tool_call') {
        return getTimelineToolArguments(event)
    }
    if (event.event_type === 'assistant_tool_result') {
        return getTimelineToolResult(event)
    }
    return ''
}

export function formatToolStepStatus(status: ToolStepStatus) {
    if (status === 'running') return '执行中'
    if (status === 'done') return '已完成'
    return '执行失败'
}

export function mapToolNameToMonitorSkill(toolName: string) {
    if (!toolName) return ''
    if (toolName.startsWith('mcp__')) {
        return renderToolLabel(toolName)
    }
    if (toolName === 'browser' || toolName === 'browser_navigate' || toolName === 'web_fetch' || toolName === 'web_search') {
        return 'agent-browser'
    }
    if (toolName === 'desktop') {
        return 'desktop-automation'
    }
    if (toolName === 'skills_install_from_repo') {
        return 'skill-installer'
    }
    return ''
}

export function isArtifactToolName(toolName: string) {
    return toolName === 'workspace_write_file' || toolName === 'workspace_edit_file' || toolName === 'desktop'
}

export function resolveArtifactAction(toolName: string) {
    if (toolName === 'workspace_edit_file') return '修改'
    if (toolName === 'workspace_write_file') return '生成'
    if (toolName === 'desktop') return '输出'
    return '产物'
}

export function extractArtifactPathFromCall(toolName: string, args: Record<string, unknown> | null) {
    if (!args) return ''
    if (toolName === 'workspace_write_file' || toolName === 'workspace_edit_file') {
        return readNestedString(args, 'path')
    }
    if (toolName === 'desktop') {
        return readNestedString(args, 'params.path')
    }
    return ''
}

export function extractArtifactPathFromResult(result: Record<string, unknown> | null) {
    if (!result) return ''
    return (
        readNestedString(result, 'path') ||
        readNestedString(result, 'file_path') ||
        readNestedString(result, 'data.path') ||
        readNestedString(result, 'result.path')
    )
}

export function buildCompactToolExecutionGroups(turnId: string, turnEvents: TimelineEvent[]) {
    const groups: CompactToolExecutionGroup[] = []
    let currentGroupEvents: TimelineEvent[] = []

    const flushGroup = () => {
        if (currentGroupEvents.length === 0) return
        const firstEventId = currentGroupEvents[0].id
        const steps = buildTurnToolExecutionSteps(currentGroupEvents)
        if (steps.length > 0) {
            groups.push({
                key: `${turnId}:${firstEventId}`,
                firstEventId,
                steps
            })
        }
        currentGroupEvents = []
    }

    for (const event of turnEvents) {
        if (isTimelineToolEvent(event)) {
            currentGroupEvents.push(event)
            continue
        }
        flushGroup()
    }

    flushGroup()
    return groups
}

export function buildTurnToolExecutionSteps(turnEvents: TimelineEvent[]): CompactToolExecutionStep[] {
    const steps: CompactToolExecutionStep[] = []
    const callIndexByToolCallId = new Map<string, number>()
    const runningIndexes: number[] = []

    for (const event of turnEvents) {
        if (!isTimelineToolEvent(event)) continue

        if (event.event_type === 'assistant_tool_call') {
            const rawName = String(getTimelinePayloadValue(event, 'name') || '')
            const normalizedToolName = normalizeToolName(rawName)
            const args = parseJsonObjectLoose(getTimelinePayloadValue(event, 'argumentsChunk'))
            const detail = getTimelineToolCompactDetail(event, turnEvents)
            const artifactPath = extractArtifactPathFromCall(normalizedToolName, args)

            steps.push({
                id: event.tool_call_id || event.id,
                toolName: normalizedToolName,
                title: getTimelineToolName(event),
                detail,
                status: 'running',
                artifactPath
            })

            const createdIndex = steps.length - 1
            runningIndexes.push(createdIndex)
            if (event.tool_call_id) {
                callIndexByToolCallId.set(event.tool_call_id, createdIndex)
            }
            continue
        }

        const resultStatus: ToolStepStatus = getTimelineToolResultStatus(event) === 'error' ? 'error' : 'done'
        const resultObj = parseJsonObjectLoose(getTimelinePayloadValue(event, 'result'))
        const resultDetail = getTimelineToolCompactDetail(event, turnEvents)
        const resultArtifactPath = extractArtifactPathFromResult(resultObj)

        let targetIndex = -1
        if (event.tool_call_id && callIndexByToolCallId.has(event.tool_call_id)) {
            targetIndex = callIndexByToolCallId.get(event.tool_call_id) ?? -1
        } else {
            for (let i = runningIndexes.length - 1; i >= 0; i -= 1) {
                const index = runningIndexes[i]
                if (steps[index] && steps[index].status === 'running') {
                    targetIndex = index
                    runningIndexes.splice(i, 1)
                    break
                }
            }
        }

        if (targetIndex >= 0 && steps[targetIndex]) {
            const step = steps[targetIndex]
            step.status = resultStatus
            step.detail = resultDetail || step.detail
            if (resultArtifactPath) {
                step.artifactPath = resultArtifactPath
            }
            const runningPosition = runningIndexes.lastIndexOf(targetIndex)
            if (runningPosition >= 0) {
                runningIndexes.splice(runningPosition, 1)
            }
            continue
        }

        const rawName = String(getTimelinePayloadValue(event, 'name') || '')
        const normalizedToolName = normalizeToolName(rawName)
        steps.push({
            id: event.tool_call_id || event.id,
            toolName: normalizedToolName,
            title: getTimelineToolName(event),
            detail: resultDetail,
            status: resultStatus,
            artifactPath: resultArtifactPath
        })
    }

    return steps
}

export function getTimelineToolCompactDetail(event: TimelineEvent, turnEvents: TimelineEvent[]) {
    const cacheKey = `${event.id}:${event.seq}`
    const cached = timelineToolCompactDetailCache.get(cacheKey)
    if (cached !== undefined) return cached

    const toolName = String(getTimelinePayloadValue(event, 'name') || '')
    const normalized = normalizeToolName(toolName)
    let detail = ''

    if (event.event_type === 'assistant_tool_call') {
        const callArgs = parseJsonObjectLoose(getTimelinePayloadValue(event, 'argumentsChunk'))
        detail = summarizeToolAction(normalized, callArgs)
        setBoundedCacheValue(timelineToolCompactDetailCache, cacheKey, detail, TOOL_DETAIL_CACHE_MAX_ENTRIES)
        return detail
    }

    if (event.event_type === 'assistant_tool_result') {
        const error = getTimelinePayloadValue(event, 'error')
        if (typeof error === 'string' && error.trim()) {
            detail = `错误: ${shortenText(error, 72)}`
            setBoundedCacheValue(timelineToolCompactDetailCache, cacheKey, detail, TOOL_DETAIL_CACHE_MAX_ENTRIES)
            return detail
        }

        const linkedCallSummary = findLinkedToolCallSummary(event, turnEvents)
        if (linkedCallSummary) {
            detail = linkedCallSummary
            setBoundedCacheValue(timelineToolCompactDetailCache, cacheKey, detail, TOOL_DETAIL_CACHE_MAX_ENTRIES)
            return detail
        }

        const resultObject = parseJsonObjectLoose(getTimelinePayloadValue(event, 'result'))
        detail = summarizeToolResult(normalized, resultObject)
        setBoundedCacheValue(timelineToolCompactDetailCache, cacheKey, detail, TOOL_DETAIL_CACHE_MAX_ENTRIES)
        return detail
    }

    setBoundedCacheValue(timelineToolCompactDetailCache, cacheKey, detail, TOOL_DETAIL_CACHE_MAX_ENTRIES)
    return detail
}

export function findLinkedToolCallSummary(event: TimelineEvent, turnEvents: TimelineEvent[]) {
    const targetToolCallId = event.tool_call_id || null
    if (!targetToolCallId) return ''

    for (let i = turnEvents.length - 1; i >= 0; i -= 1) {
        const candidate = turnEvents[i]
        if (candidate.id === event.id) continue
        if (candidate.event_type !== 'assistant_tool_call') continue
        if (candidate.tool_call_id !== targetToolCallId) continue
        const callArgs = parseJsonObjectLoose(getTimelinePayloadValue(candidate, 'argumentsChunk'))
        const toolName = String(getTimelinePayloadValue(candidate, 'name') || '')
        const summary = summarizeToolAction(normalizeToolName(toolName), callArgs)
        if (summary) return summary
    }
    return ''
}

export function summarizeToolAction(toolName: string, args: Record<string, unknown> | null) {
    if (!args) return ''

    const pick = (...keys: string[]) => {
        for (const key of keys) {
            const value = readNestedString(args, key)
            if (value) return value
        }
        return ''
    }

    if (toolName === 'browser') {
        const action = pick('action')
        const url = pick('params.url', 'url')
        const selector = pick('params.selector', 'selector')
        if (action === 'navigate' && url) return `打开链接: ${shortenUrl(url)}`
        if (action === 'click' && selector) return `点击元素: ${shortenText(selector, 52)}`
        if (action === 'type') {
            const target = selector || pick('params.text', 'text')
            if (target) return `输入: ${shortenText(target, 52)}`
        }
        if (action) return `浏览器动作: ${action}`
    }

    if (toolName === 'desktop') {
        const action = pick('action')
        const windowId = pick('params.window_id', 'params.id', 'params.hwnd')
        const controlId = pick('params.control_id', 'params.id')
        const text = pick('params.text')
        const path = pick('params.path')
        if (action === 'select_window' && windowId) return `选择窗口: ${windowId}`
        if (action === 'launch_application') {
            const command = pick('params.command')
            if (command) return `启动程序: ${shortenText(command, 56)}`
        }
        if (action === 'click_input' && controlId) return `点击控件: ${controlId}`
        if (action === 'set_edit_text') {
            if (text) return `输入文本: ${shortenText(text, 40)}`
            if (controlId) return `设置控件文本: ${controlId}`
        }
        if (action === 'keyboard_input') {
            const keys = pick('params.keys')
            if (keys) return `键盘输入: ${shortenText(keys, 52)}`
        }
        if (action === 'capture_window_screenshot' || action === 'capture_desktop_screenshot') {
            return '截取桌面截图'
        }
        if (action && path) return `${action}: ${shortenPath(path)}`
        if (action) return `桌面动作: ${action}`
    }

    if (toolName === 'browser_navigate') {
        const url = pick('url')
        if (url) return `打开链接: ${shortenUrl(url)}`
    }

    if (toolName === 'web_fetch') {
        const url = pick('url')
        if (url) return `抓取网页: ${shortenUrl(url)}`
    }

    if (toolName === 'web_search') {
        const query = pick('query', 'q')
        if (query) return `搜索: ${shortenText(query, 56)}`
    }

    if (toolName === 'bash') {
        const command = pick('command')
        if (command) return `执行命令: ${shortenText(command, 68)}`
    }

    if (toolName === 'workspace_list_directory') {
        const path = pick('path')
        if (path) return `查看目录: ${shortenPath(path)}`
        return '查看目录内容'
    }

    if (toolName === 'workspace_read_file') {
        const path = pick('path')
        if (path) return `读取文件: ${shortenPath(path)}`
    }

    if (toolName === 'workspace_write_file' || toolName === 'workspace_edit_file') {
        const path = pick('path')
        if (path) return `写入文件: ${shortenPath(path)}`
    }

    if (toolName === 'workspace_grep') {
        const pattern = pick('pattern')
        const path = pick('path')
        if (pattern && path) return `搜索 "${shortenText(pattern, 28)}" 于 ${shortenPath(path)}`
        if (pattern) return `搜索内容: ${shortenText(pattern, 56)}`
    }

    if (toolName === 'workspace_glob') {
        const pattern = pick('pattern')
        const path = pick('path')
        if (pattern && path) return `匹配 ${shortenText(pattern, 32)} 于 ${shortenPath(path)}`
        if (pattern) return `匹配文件: ${shortenText(pattern, 56)}`
    }

    if (toolName === 'skills_install_from_repo') {
        const repo = pick('repo_url', 'repoUrl')
        if (repo) return `安装技能: ${shortenUrl(repo)}`
    }

    const fallback = firstObjectEntrySummary(args)
    return fallback ? `参数: ${fallback}` : ''
}

export function summarizeToolResult(toolName: string, result: Record<string, unknown> | null) {
    if (!result) return ''

    const pick = (...keys: string[]) => {
        for (const key of keys) {
            const value = readNestedString(result, key)
            if (value) return value
        }
        return ''
    }

    if (toolName === 'browser' || toolName === 'browser_navigate') {
        const url = pick('url', 'current_url', 'final_url')
        if (url) return `页面: ${shortenUrl(url)}`
    }

    if (toolName === 'web_fetch') {
        const url = pick('url')
        const status = pick('status', 'status_code')
        if (url && status) return `抓取完成: ${shortenUrl(url)} (${status})`
        if (url) return `抓取完成: ${shortenUrl(url)}`
    }

    if (toolName === 'web_search') {
        const query = pick('query', 'q')
        if (query) return `搜索完成: ${shortenText(query, 56)}`
    }

    if (toolName === 'desktop') {
        const ok = pick('ok')
        const path = pick('data.path')
        if (ok === 'true' && path) return `截图: ${shortenPath(path)}`
        const selectedWindow = pick('data.selected_window.title')
        if (selectedWindow) return `窗口: ${shortenText(selectedWindow, 56)}`
        const error = pick('error')
        if (error) return `失败: ${shortenText(error, 56)}`
    }

    const fallback = firstObjectEntrySummary(result)
    return fallback ? `结果: ${fallback}` : ''
}

export function parseJsonObjectLoose(value: unknown): Record<string, unknown> | null {
    if (!value) return null
    if (typeof value === 'object' && !Array.isArray(value)) {
        return value as Record<string, unknown>
    }
    if (typeof value !== 'string') return null

    let candidate = value.trim()
    if (!candidate) return null

    for (let i = 0; i < 3; i += 1) {
        try {
            const parsed = JSON.parse(candidate)
            if (parsed && typeof parsed === 'object' && !Array.isArray(parsed)) {
                return parsed as Record<string, unknown>
            }
            if (typeof parsed === 'string') {
                const next = parsed.trim()
                if (!next || next === candidate) break
                candidate = next
                continue
            }
            break
        } catch {
            break
        }
    }
    return null
}

export function readNestedString(source: Record<string, unknown>, path: string) {
    const parts = path.split('.')
    let current: unknown = source
    for (const part of parts) {
        if (!current || typeof current !== 'object' || Array.isArray(current)) return ''
        current = (current as Record<string, unknown>)[part]
    }
    if (typeof current === 'string') {
        const trimmed = current.trim()
        return trimmed || ''
    }
    if (typeof current === 'number' || typeof current === 'boolean') {
        return String(current)
    }
    return ''
}

export function firstObjectEntrySummary(source: Record<string, unknown>) {
    const entries = Object.entries(source)
    for (const [key, raw] of entries) {
        if (raw === null || raw === undefined) continue
        if (typeof raw === 'object') continue
        const text = String(raw).trim()
        if (!text) continue
        return `${key}=${shortenText(text, 48)}`
    }
    return ''
}

export function shortenText(value: string, maxLength: number) {
    const normalized = value.replace(/\s+/g, ' ').trim()
    if (normalized.length <= maxLength) return normalized
    return `${normalized.slice(0, Math.max(0, maxLength - 1))}...`
}

export function shortenUrl(url: string) {
    return truncateMiddle(url, 72)
}

export function shortenPath(path: string) {
    return truncateMiddle(path, 68)
}

export function getPathName(path: string) {
    const parts = path.split(/[/\\]/)
    const name = parts[parts.length - 1]
    if (!name.includes('.')) return name
    const dot = name.lastIndexOf('.')
    return name.slice(dot + 1)
}
