<template>
  <div
    class="message-list no-scrollbar"
    @click="handleMarkdownLinkClick"
    @scroll.passive="handleMessageListScroll"
  >
    <div v-if="isLegacyTimeline" class="empty-tip">Legacy 会话：按近似顺序回放</div>
    <template v-for="turn in turns" :key="turn.turnId">
      <div v-if="turn.userText" class="message-row user">
        <div class="message-meta user-meta">
          <span class="time">{{ formatTime(turn.userCreatedAt) }}</span>
          <span class="name">{{ userName }}</span>
          <div class="message-avatar">
            <img class="avatar-img" :src="userAvatar" alt="User Avatar" />
          </div>
        </div>
        <div class="bubble">
          <div v-html="renderMarkdown(turn.userText)"></div>
        </div>
      </div>

      <div v-if="turn.assistantEvents.length > 0" class="message-row assistant">
        <div class="message-meta">
          <span class="name">Petool</span>
          <span class="time">{{ formatTime(turn.assistantCreatedAt) }}</span>
        </div>
        <div class="bubble">
          <div
            v-for="event in turn.assistantEvents"
            :key="event.id"
            class="timeline-event"
          >
            <template v-if="event.event_type === 'assistant_reasoning'">
              <div class="reasoning">
                <button class="reasoning-toggle" @click="toggleTimelineReasoning(event.id)">
                  <span>思考过程</span>
                  <span class="reasoning-state">{{ isTimelineReasoningCollapsed(event.id) ? '已折叠' : '展开中' }}</span>
                  <span class="material-icons-round">
                    {{ isTimelineReasoningCollapsed(event.id) ? 'expand_more' : 'expand_less' }}
                  </span>
                </button>
                <div v-show="!isTimelineReasoningCollapsed(event.id)" class="reasoning-content">
                  {{ getTimelineReasoningText(event) }}
                </div>
              </div>
            </template>

            <template v-else-if="isTimelineToolEvent(event)">
              <div v-if="isToolDisplayFull" class="tool-progress">
                <div class="tool-list">
                  <div class="tool-item" :class="event.event_type === 'assistant_tool_call' ? 'running' : getTimelineToolResultStatus(event)">
                    <div class="tool-title">{{ getTimelineToolName(event) }}</div>
                    <div v-if="getTimelineToolDisplayText(event)" class="tool-text">
                      <span class="tool-text-label">{{ event.event_type === 'assistant_tool_call' ? '参数' : '结果' }}</span>
                      <pre class="tool-code">{{ getTimelineToolDisplayText(event) }}</pre>
                    </div>
                  </div>
                </div>
              </div>
              <div
                v-else-if="shouldRenderCompactToolSummary(turn.turnId, event.id)"
                class="tool-compact-batch"
              >
                <button class="tool-batch-toggle" @click="toggleToolExecutionGroup(turn.turnId, event.id)">
                  <span class="material-icons-round">terminal</span>
                  <span class="tool-batch-title">查看执行工具</span>
                  <span class="tool-batch-count">{{ getToolExecutionGroupSteps(turn.turnId, event.id).length }} step</span>
                  <span class="material-icons-round">
                    {{ isToolExecutionGroupCollapsed(turn.turnId, event.id) ? 'expand_more' : 'expand_less' }}
                  </span>
                </button>
                <div v-show="!isToolExecutionGroupCollapsed(turn.turnId, event.id)" class="tool-batch-list">
                  <div
                    v-for="step in getToolExecutionGroupSteps(turn.turnId, event.id)"
                    :key="step.id"
                    class="tool-batch-item"
                    :class="step.status"
                  >
                    <span class="status-indicator" :class="step.status" aria-hidden="true">
                      <span v-if="step.status === 'running'" class="status-spinner"></span>
                      <span v-else-if="step.status === 'done'" class="material-icons-round">check_circle</span>
                      <span v-else class="material-icons-round">cancel</span>
                    </span>
                    <div class="tool-batch-main">
                      <div class="tool-batch-line">
                        <span class="tool-batch-name">{{ step.title }}</span>
                        <span class="tool-batch-status">{{ formatToolStepStatus(step.status) }}</span>
                      </div>
                      <div v-if="step.detail" class="tool-batch-detail">{{ step.detail }}</div>
                    </div>
                  </div>
                </div>
              </div>
            </template>

            <template v-else-if="event.event_type === 'assistant_text'">
              <div v-html="renderMarkdown(getTimelineText(event))"></div>
            </template>
          </div>
        </div>
      </div>
    </template>

    <div v-if="shouldShowStandaloneTypingBubble" class="message-row assistant">
      <div class="message-meta">
        <span class="name">Petool</span>
        <span class="time">{{ formatTime(new Date().toISOString()) }}</span>
      </div>
      <div class="bubble">
        <div class="typing"><span></span><span></span><span></span></div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { marked } from 'marked'
import { ElMessage } from 'element-plus'
import {
  type TimelineTurnDisplay,
  type CompactToolExecutionGroup,
  setBoundedCacheValue,
  getTimelineReasoningText,
  isTimelineToolEvent,
  getTimelineToolName,
  getTimelineToolDisplayText,
  getTimelineToolResultStatus,
  formatToolStepStatus,
  getTimelineText
} from '@/utils/timeline-formatter'

const MARKDOWN_CACHE_MAX_ENTRIES = 300
const markdownCache = new Map<string, string>()

const props = defineProps<{
  turns: TimelineTurnDisplay[]
  groupsByTurnId: Record<string, CompactToolExecutionGroup[]>
  isToolDisplayFull: boolean
  isLegacyTimeline: boolean
  shouldShowStandaloneTypingBubble: boolean
  userName: string
  userAvatar: string
}>()

const emit = defineEmits<{
  (e: 'scroll', element: HTMLElement): void
}>()

const timelineReasoningCollapsedByEventId = ref<Record<string, boolean>>({})
const compactToolExecutionCollapsedByGroupKey = ref<Record<string, boolean>>({})

function formatTime(isoString: string) {
  if (!isoString) return ''
  const date = new Date(isoString)
  return date.toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' })
}

function getToolExecutionGroup(turnId: string, firstEventId: string) {
  const groups = props.groupsByTurnId[turnId] || []
  for (const group of groups) {
    if (group.firstEventId === firstEventId) return group
  }
  return null
}

function getToolExecutionGroupKey(turnId: string, firstEventId: string) {
  const group = getToolExecutionGroup(turnId, firstEventId)
  return group?.key || `${turnId}:${firstEventId}`
}

function isToolExecutionGroupCollapsed(turnId: string, firstEventId: string) {
  const key = getToolExecutionGroupKey(turnId, firstEventId)
  return compactToolExecutionCollapsedByGroupKey.value[key] ?? true
}

function toggleToolExecutionGroup(turnId: string, firstEventId: string) {
  const key = getToolExecutionGroupKey(turnId, firstEventId)
  compactToolExecutionCollapsedByGroupKey.value[key] = !isToolExecutionGroupCollapsed(turnId, firstEventId)
}

function getToolExecutionGroupSteps(turnId: string, firstEventId: string) {
  const group = getToolExecutionGroup(turnId, firstEventId)
  return group?.steps || []
}

function shouldRenderCompactToolSummary(turnId: string, firstEventId: string) {
  if (props.isToolDisplayFull) return false
  const group = getToolExecutionGroup(turnId, firstEventId)
  return Boolean(group && group.steps.length > 0)
}

function isTimelineReasoningCollapsed(eventId: string) {
  return timelineReasoningCollapsedByEventId.value[eventId] ?? false
}

function toggleTimelineReasoning(eventId: string) {
  timelineReasoningCollapsedByEventId.value[eventId] = !isTimelineReasoningCollapsed(eventId)
}

// Markdown Rendering Logic
function escapeHtml(value: string) {
  return value
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;')
}

function isSvgCodeBlock(code: string, infostring?: string) {
  const lang = (infostring || '').trim().toLowerCase()
  if (lang === 'svg' || lang === 'image/svg+xml') return true
  const trimmed = code.trim()
  return /^<svg[\s>]/i.test(trimmed) && /<\/svg>\s*$/i.test(trimmed)
}

const markdownRenderer = new marked.Renderer()
markdownRenderer.code = (code: string, infostring: string | undefined, escaped: boolean) => {
  const language = (infostring || '').trim()
  const languageClass = language ? ` class="language-${escapeHtml(language)}"` : ''
  const safeCode = escaped ? code : escapeHtml(code)
  const codeBlockHtml = `
<div class="md-code-block">
  <button class="md-code-copy-btn" type="button" data-action="copy-code">复制</button>
  <pre><code${languageClass}>${safeCode}</code></pre>
</div>`
  if (!isSvgCodeBlock(code, infostring)) {
    return codeBlockHtml
  }

  const svgSource = code.trim()
  const previewSrc = `data:image/svg+xml;utf8,${encodeURIComponent(svgSource)}`
  return `
<div class="svg-preview-block">
  ${codeBlockHtml}
  <div class="svg-preview-label">SVG 预览</div>
  <div class="svg-preview-canvas">
    <img class="svg-preview-image" src="${previewSrc}" alt="SVG 预览" loading="lazy" />
  </div>
</div>`
}

function renderMarkdown(content: string) {
  const source = content || ''
  const cached = markdownCache.get(source)
  if (cached !== undefined) {
    return cached
  }
  const rendered = marked.parse(source, { async: false, renderer: markdownRenderer }) as string
  setBoundedCacheValue(markdownCache, source, rendered, MARKDOWN_CACHE_MAX_ENTRIES)
  return rendered
}

function isExternalHttpUrl(value: string) {
  return /^https?:\/\//i.test(value)
}

function getErrorMessage(error: unknown, fallback: string): string {
  if (error instanceof Error) return error.message
  if (typeof error === 'string') return error
  return error && typeof error === 'object' && 'message' in error ? String((error as any).message) : fallback
}

async function openExternal(url: string) {
  try {
    const { open } = await import('@tauri-apps/plugin-shell')
    await open(url)
  } catch {
    window.open(url, '_blank')
  }
}

async function copyTextToClipboard(text: string) {
  if (!text) return false
  try {
    await navigator.clipboard.writeText(text)
    return true
  } catch {
    const textarea = document.createElement('textarea')
    textarea.value = text
    textarea.style.position = 'fixed'
    textarea.style.opacity = '0'
    textarea.style.pointerEvents = 'none'
    document.body.appendChild(textarea)
    textarea.focus()
    textarea.select()
    let copied = false
    try {
      copied = document.execCommand('copy')
    } catch {
      copied = false
    }
    document.body.removeChild(textarea)
    return copied
  }
}

async function handleMarkdownLinkClick(event: MouseEvent) {
  const target = event.target
  if (!(target instanceof HTMLElement)) return

  const copyTrigger = target.closest('[data-action="copy-code"]') as HTMLButtonElement | null
  if (copyTrigger) {
    event.preventDefault()
    event.stopPropagation()
    const block = copyTrigger.closest('.md-code-block') as HTMLElement | null
    const codeElement = block?.querySelector('code') as HTMLElement | null
    const content = codeElement?.textContent || ''
    if (!content.trim()) {
      ElMessage.warning('没有可复制的代码')
      return
    }
    const copied = await copyTextToClipboard(content)
    if (!copied) {
      ElMessage.error('复制失败')
      return
    }
    copyTrigger.textContent = '已复制'
    copyTrigger.classList.add('copied')
    window.setTimeout(() => {
      copyTrigger.textContent = '复制'
      copyTrigger.classList.remove('copied')
    }, 1200)
    return
  }

  const anchor = target.closest('a') as HTMLAnchorElement | null
  if (!anchor) return

  const href = (anchor.getAttribute('href') || '').trim()
  if (!href || !isExternalHttpUrl(href)) return

  event.preventDefault()
  event.stopPropagation()

  try {
    await openExternal(href)
  } catch (error) {
    ElMessage.error(getErrorMessage(error, '打开外部链接失败'))
  }
}

function handleMessageListScroll(event: Event) {
  if (event.target instanceof HTMLElement) {
    emit('scroll', event.target)
  }
}
</script>

<style scoped>
.message-list {
  flex: 1;
  min-height: 0;
  width: 100%;
  max-width: 900px;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: 32px;
  contain: layout paint style;
  padding: 20px 0;
}

.message-row {
  max-width: 84%;
  display: flex;
  flex-direction: column;
  gap: 2px;
  content-visibility: auto;
  contain-intrinsic-size: 160px;
}

.message-row.assistant {
  align-self: flex-start;
}

.message-row.user {
  align-self: flex-end;
  width: 100%;
  max-width: 100%;
  align-items: flex-end;
  justify-content: flex-end;
}

.message-meta {
  margin-bottom: 4px;
  padding-left: 0;
  display: flex;
  gap: 8px;
  align-items: center;
}

.user-meta {
  justify-content: flex-end;
  width: 100%;
  gap: 10px;
  padding-left: 0;
  padding-right: 0;
}

.user-meta .time {
  margin-right: 2px;
}

.message-avatar {
  width: 40px;
  height: 40px;
  border-radius: 999px;
  overflow: hidden;
  border: 2px solid #f5f3ee;
  box-shadow: 0 6px 12px -8px rgba(0, 0, 0, 0.45);
  flex-shrink: 0;
}

.avatar-img {
  width: 100%;
  height: 100%;
  object-fit: cover;
}

.name {
  font-size: 12px;
  color: #4a7c59;
  font-weight: 700;
}

.time {
  font-size: 10px;
  color: #9ca3af;
}

.reasoning {
  border: 1px dashed #d6d3d1;
  border-radius: 10px;
  margin-top: 8px;
  background: #fafaf9;
}

.reasoning-toggle {
  width: 100%;
  border: none;
  background: transparent;
  display: flex;
  gap: 8px;
  align-items: center;
  padding: 8px;
  cursor: pointer;
  font-size: 12px;
  font-weight: 700;
  color: #57534e;
}

.reasoning-state {
  margin-left: auto;
  color: #9ca3af;
  font-weight: 600;
}

.reasoning-content {
  border-top: 1px dashed #e7e5e4;
  padding: 8px;
  font-size: 12px;
  color: #78716c;
  white-space: pre-wrap;
}

.bubble {
  border-radius: 28px;
  padding: 8px 16px;
  font-size: 13px;
  line-height: 2.4;
  color: #44403c;
  word-break: break-word;
}

.message-row.assistant .bubble {
  background: transparent;
  border: none;
  padding: 4px 0;
}

.message-row.user .bubble {
  background: #F3F4F6;
  border: none;
  border-radius: 20px;
  border-top-right-radius: 4px;
  margin-right: 52px;
  max-width: min(85%, 720px);
}

.bubble :deep(pre) {
  margin: 8px 0;
  border-radius: 10px;
  background: #1f2937;
  color: #f9fafb;
  padding: 12px;
  overflow-x: auto;
}

.bubble :deep(code) {
  font-family: Consolas, Monaco, monospace;
  font-size: 12px;
}

.bubble :deep(.md-code-block) {
  position: relative;
  margin: 8px 0;
}

.bubble :deep(.md-code-block > pre) {
  margin: 0;
  padding-top: 36px;
}

.bubble :deep(.md-code-copy-btn) {
  position: absolute;
  top: 8px;
  right: 8px;
  z-index: 2;
  border: 1px solid rgba(148, 163, 184, 0.4);
  background: rgba(30, 41, 59, 0.92);
  color: #e2e8f0;
  border-radius: 6px;
  font-size: 11px;
  line-height: 1;
  padding: 6px 8px;
  cursor: pointer;
  transition: background-color 0.15s ease, color 0.15s ease, border-color 0.15s ease;
}

.bubble :deep(.md-code-copy-btn:hover) {
  background: rgba(51, 65, 85, 0.95);
  border-color: rgba(148, 163, 184, 0.75);
}

.bubble :deep(.md-code-copy-btn.copied) {
  background: rgba(22, 101, 52, 0.92);
  border-color: rgba(74, 222, 128, 0.7);
  color: #dcfce7;
}

.bubble :deep(.svg-preview-block) {
  margin: 8px 0;
}

.bubble :deep(.svg-preview-block .md-code-block) {
  margin: 0;
}

.bubble :deep(.svg-preview-label) {
  margin-top: 8px;
  font-size: 12px;
  font-weight: 700;
  color: #57534e;
}

.bubble :deep(.svg-preview-canvas) {
  margin-top: 6px;
  border: 1px solid #e7e5e4;
  border-radius: 10px;
  background: #fafaf9;
  padding: 12px;
  display: flex;
  align-items: center;
  justify-content: center;
  overflow: auto;
}

.bubble :deep(.svg-preview-image) {
  display: block;
  max-width: min(100%, 320px);
  max-height: 320px;
  width: auto;
  height: auto;
}

.tool-progress {
  margin-top: 8px;
}

.tool-list {
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.tool-item {
  border: 1px solid #e7e5e4;
  border-radius: 10px;
  padding: 8px;
}

.tool-item.done {
  border-color: #4a7c59;
}

.tool-item.error {
  border-color: #dc2626;
}

.tool-title {
  font-size: 12px;
  font-weight: 700;
}

.tool-text {
  margin-top: 4px;
  font-size: 12px;
  color: #78716c;
  white-space: pre-wrap;
  word-break: break-word;
}

.tool-text-label {
  display: inline-flex;
  align-items: center;
  margin-right: 6px;
  padding: 0 6px;
  border-radius: 999px;
  font-size: 10px;
  font-weight: 700;
  color: #64748b;
  background: #f1f5f9;
}

.tool-text .tool-code {
  margin: 6px 0 0;
  padding: 8px 10px;
  border-radius: 8px;
  border: 1px solid #e2e8f0;
  background: #f8fafc;
  color: #334155;
  font-family: Consolas, Monaco, monospace;
  font-size: 12px;
  line-height: 1.45;
  white-space: pre-wrap;
  overflow-x: auto;
}

.tool-compact-batch {
  margin-top: 8px;
  border: 1px solid #e6e9e2;
  border-radius: 12px;
  background: #fbfdfb;
  overflow: hidden;
}

.tool-batch-toggle {
  width: 100%;
  border: none;
  background: transparent;
  display: flex;
  align-items: center;
  gap: 8px;
  padding: 9px 10px;
  cursor: pointer;
  color: #4f5f53;
}

.tool-batch-toggle .material-icons-round {
  font-size: 16px;
  color: #94a3b8;
}

.tool-batch-title {
  font-size: 12px;
  font-weight: 800;
}

.tool-batch-count {
  margin-left: auto;
  font-size: 11px;
  color: #9ca3af;
  font-weight: 700;
}

.tool-batch-list {
  border-top: 1px dashed #e7e5e4;
  padding: 8px 10px;
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.tool-batch-item {
  display: flex;
  gap: 8px;
  align-items: flex-start;
}

.tool-batch-main {
  flex: 1;
  min-width: 0;
  display: flex;
  flex-direction: column;
  gap: 2px;
}

.tool-batch-line {
  display: flex;
  align-items: center;
  gap: 8px;
}

.tool-batch-name {
  font-size: 12px;
  color: #44403c;
  font-weight: 700;
}

.tool-batch-status {
  margin-left: auto;
  font-size: 11px;
  color: #94a3b8;
  font-weight: 700;
}

.tool-batch-detail {
  font-size: 11px;
  color: #6b7280;
  line-height: 1.45;
  word-break: break-word;
}

.status-indicator {
  width: 16px;
  height: 16px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
  margin-top: 1px;
}

.status-indicator .material-icons-round {
  font-size: 16px;
  line-height: 1;
}

.status-indicator.running {
  color: #3b82f6;
}

.status-indicator.done {
  color: #4a7c59;
}

.status-indicator.error {
  color: #dc2626;
}

.status-spinner {
  width: 12px;
  height: 12px;
  border-radius: 999px;
  border: 2px solid rgba(59, 130, 246, 0.2);
  border-top-color: #3b82f6;
  animation: toolSpin 0.9s linear infinite;
}

.tool-batch-item.done .status-indicator .material-icons-round {
  animation: statusPop 0.25s ease-out;
}

.typing {
  display: flex;
  gap: 4px;
  padding-top: 6px;
}

.typing span {
  width: 8px;
  height: 8px;
  border-radius: 999px;
  background: #9ca3af;
  animation: typing 1.4s infinite;
}

.typing span:nth-child(2) {
  animation-delay: 0.2s;
}

.typing span:nth-child(3) {
  animation-delay: 0.4s;
}

.empty-tip {
  color: #a8a29e;
  font-size: 12px;
  text-align: center;
  padding-top: 24px;
}

.no-scrollbar::-webkit-scrollbar {
  display: none;
}

.no-scrollbar {
  -ms-overflow-style: none;
  scrollbar-width: none;
}

@keyframes toolSpin {
  from {
    transform: rotate(0deg);
  }

  to {
    transform: rotate(360deg);
  }
}

@keyframes statusPop {
  0% {
    transform: scale(0.75);
    opacity: 0.7;
  }

  100% {
    transform: scale(1);
    opacity: 1;
  }
}

@keyframes typing {

  0%,
  60%,
  100% {
    transform: translateY(0);
    opacity: 0.6;
  }

  30% {
    transform: translateY(-7px);
    opacity: 1;
  }
}
</style>
