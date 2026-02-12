import { ref, type Ref } from 'vue'
import type { Message } from '../stores/chat'
import type { ReasoningEntry, ToolStreamItem } from './useChatEventBridge'
import {
  extractJsonLikeStringField,
  formatToolPayload,
  normalizeToolName,
  parseJsonRecord,
  renderToolLabel,
  truncateMiddle
} from '../utils/toolDisplay'

interface ChatStoreLike {
  streaming: boolean
  messages: Record<string, Message[]>
}

interface UseChatMessageArtifactsOptions {
  chatStore: ChatStoreLike
  activeAssistantMessageId: Ref<string | null>
  toolStepLabels: Record<string, string>
}

export function useChatMessageArtifacts(options: UseChatMessageArtifactsOptions) {
  const reasoningByMessage = ref<Record<string, ReasoningEntry>>({})
  const toolStreamItems = ref<ToolStreamItem[]>([])
  const toolHistoryByMessage = ref<Record<string, ToolStreamItem[]>>({})
  const toolListCollapsedByMessage = ref<Record<string, boolean>>({})
  const hiddenAssistantMessages = ref<Record<string, boolean>>({})
  const assistantContentOverrides = ref<Record<string, string>>({})

  function initializeAssistantArtifacts(messageId: string) {
    toolHistoryByMessage.value[messageId] = []
    toolListCollapsedByMessage.value[messageId] = false
    hiddenAssistantMessages.value[messageId] = false
    delete assistantContentOverrides.value[messageId]
  }

  function clearAssistantArtifacts(messageId: string) {
    delete toolHistoryByMessage.value[messageId]
    delete toolListCollapsedByMessage.value[messageId]
    delete hiddenAssistantMessages.value[messageId]
    delete assistantContentOverrides.value[messageId]
  }

  function collapseActiveReasoning() {
    if (!options.activeAssistantMessageId.value) return
    const entry = reasoningByMessage.value[options.activeAssistantMessageId.value]
    if (!entry || !entry.text.trim()) return
    entry.collapsed = true
  }

  function getReasoningEntry(messageId: string) {
    return reasoningByMessage.value[messageId]
  }

  function toggleReasoning(messageId: string) {
    const entry = reasoningByMessage.value[messageId]
    if (!entry) return
    entry.collapsed = !entry.collapsed
  }

  function isStreamingMessage(messageId: string) {
    return Boolean(options.chatStore.streaming && options.activeAssistantMessageId.value === messageId)
  }

  function isRenderableMessage(message: Message) {
    if (message.role === 'user') return true
    if (message.role !== 'assistant') return false
    if (hiddenAssistantMessages.value[message.id]) return false
    if (isStreamingMessage(message.id)) return true

    const hasContent = Boolean(getDisplayedMessageContent(message).trim())
    const hasReasoning = Boolean(getReasoningEntry(message.id)?.text.trim())
    const hasToolCalls = Array.isArray(message.tool_calls) && message.tool_calls.length > 0
    const hasToolHistory = getToolItemsForMessage(message.id).length > 0
    return hasContent || hasReasoning || hasToolCalls || hasToolHistory
  }

  function hydrateConversationArtifacts(conversationId: string) {
    const messages = options.chatStore.messages[conversationId] || []
    const hydratedReasoning: Record<string, ReasoningEntry> = {}
    const hydratedToolHistory: Record<string, ToolStreamItem[]> = {}
    const hydratedCollapseMap: Record<string, boolean> = {}
    const nextHiddenMessages: Record<string, boolean> = {}
    const nextContentOverrides: Record<string, string> = {}

    let index = 0
    while (index < messages.length) {
      const message = messages[index]
      if (message.role !== 'assistant') {
        index += 1
        continue
      }

      const toolCalls = extractMessageToolCalls(message)
      const reasoningText = normalizeReasoningText(message.reasoning)

      if (toolCalls.length === 0) {
        if (reasoningText) {
          hydratedReasoning[message.id] = {
            text: reasoningText,
            collapsed: true
          }
        }
        index += 1
        continue
      }

      let segmentEnd = index + 1
      while (segmentEnd < messages.length) {
        const next = messages[segmentEnd]
        if (next.role === 'user' || next.role === 'system') break
        segmentEnd += 1
      }

      const rootAssistantId = message.id
      const mergedReasoningParts: string[] = []
      let mergedContent = ''
      const mergedToolItems: ToolStreamItem[] = []
      const mergedToolItemsById = new Map<string, ToolStreamItem>()

      for (let cursor = index; cursor < segmentEnd; cursor += 1) {
        const segmentMessage = messages[cursor]
        if (segmentMessage.role === 'assistant') {
          if (segmentMessage.id !== rootAssistantId) {
            nextHiddenMessages[segmentMessage.id] = true
          }

          const assistantReasoning = normalizeReasoningText(segmentMessage.reasoning)
          if (assistantReasoning) {
            mergedReasoningParts.push(assistantReasoning)
          }

          if (segmentMessage.content.trim()) {
            mergedContent = segmentMessage.content
          }

          const segmentToolCalls = extractMessageToolCalls(segmentMessage)
          for (const call of segmentToolCalls) {
            const id =
              (typeof call?.id === 'string' && call.id.trim()) ||
              `${segmentMessage.id}-tool-${mergedToolItems.length}`
            if (mergedToolItemsById.has(id)) continue

            const name =
              (typeof call?.tool_name === 'string' && call.tool_name.trim()) ||
              (typeof call?.toolName === 'string' && call.toolName.trim()) ||
              'tool'
            const item: ToolStreamItem = {
              id,
              name,
              arguments: serializeToolArguments(call?.arguments),
              result: '',
              status: 'running',
              index: mergedToolItems.length
            }
            mergedToolItemsById.set(id, item)
            mergedToolItems.push(item)
          }
          continue
        }

        if (segmentMessage.role !== 'tool') continue

        const toolMeta = extractMessageToolMeta(segmentMessage)
        let target = toolMeta.id ? mergedToolItemsById.get(toolMeta.id) : undefined
        if (!target && toolMeta.name) {
          target = mergedToolItems.find((item) => item.name === toolMeta.name && item.status === 'running')
        }
        if (!target) {
          target = mergedToolItems.find((item) => item.status === 'running') || mergedToolItems[mergedToolItems.length - 1]
        }
        if (!target) continue

        if (toolMeta.name) {
          target.name = toolMeta.name
        }
        target.result = segmentMessage.content || ''
        target.status = inferToolResultStatus(target.result)
      }

      if (mergedReasoningParts.length > 0) {
        hydratedReasoning[rootAssistantId] = {
          text: mergeReasoningParts(mergedReasoningParts),
          collapsed: true
        }
      }

      if (mergedToolItems.length > 0) {
        hydratedToolHistory[rootAssistantId] = mergedToolItems
        hydratedCollapseMap[rootAssistantId] = false
      }

      if (mergedContent.trim()) {
        nextContentOverrides[rootAssistantId] = mergedContent
      }

      index = segmentEnd
    }

    reasoningByMessage.value = hydratedReasoning
    toolHistoryByMessage.value = hydratedToolHistory
    toolListCollapsedByMessage.value = hydratedCollapseMap
    hiddenAssistantMessages.value = nextHiddenMessages
    assistantContentOverrides.value = nextContentOverrides
  }

  function shouldShowMessageBubble(message: Message) {
    if (message.role === 'user') return true
    const renderedContent = getDisplayedMessageContent(message).trim()
    const hasReasoning = Boolean(getReasoningEntry(message.id)?.text.trim())
    return Boolean(
      renderedContent ||
        isStreamingMessage(message.id) ||
        getToolItemsForMessage(message.id).length > 0 ||
        hasReasoning
    )
  }

  function shouldRenderAssistantContent(message: Message) {
    if (!getDisplayedMessageContent(message).trim()) return false
    if (message.role !== 'assistant') return true
    if (!isStreamingMessage(message.id)) return true

    const hasToolProgress = getToolItemsForMessage(message.id).length > 0
    const hasReasoning = Boolean(getReasoningEntry(message.id)?.text.trim())
    if (hasToolProgress || hasReasoning) return false
    return true
  }

  function persistToolItemsForActiveMessage() {
    const messageId = options.activeAssistantMessageId.value
    if (!messageId) {
      toolStreamItems.value = []
      return
    }

    if (toolStreamItems.value.length === 0) {
      delete toolHistoryByMessage.value[messageId]
      delete toolListCollapsedByMessage.value[messageId]
      toolStreamItems.value = []
      return
    }

    toolHistoryByMessage.value[messageId] = toolStreamItems.value.map((item) => ({
      id: item.id,
      name: item.name,
      arguments: item.arguments,
      result: item.result,
      status: item.status,
      index: item.index
    }))
    toolStreamItems.value = []
  }

  function getToolItemsForMessage(messageId: string) {
    if (isStreamingMessage(messageId)) return toolStreamItems.value
    return toolHistoryByMessage.value[messageId] || []
  }

  function shouldShowToolProgress(message: Message) {
    if (message.role !== 'assistant') return false
    return getToolItemsForMessage(message.id).length > 0
  }

  function isToolListCollapsed(messageId: string) {
    return toolListCollapsedByMessage.value[messageId] ?? false
  }

  function toggleCompactToolList(messageId: string) {
    toolListCollapsedByMessage.value[messageId] = !isToolListCollapsed(messageId)
  }

  function getCompactToolSummaryTitle(messageId: string) {
    const { total, running, error } = getToolStats(getToolItemsForMessage(messageId))
    if (total === 0) return '执行步骤'
    if (error > 0) return '执行出现异常'
    if (running > 0) return '正在执行步骤'
    return '步骤执行完成'
  }

  function getCompactToolSummaryCount(messageId: string) {
    const { done, total } = getToolStats(getToolItemsForMessage(messageId))
    return `${done}/${total}`
  }

  function renderToolItemName(item: ToolStreamItem) {
    const base = renderToolStepName(item.name)
    const detail = extractToolStepDetail(item)
    if (!detail) return base
    return `${base} · ${detail}`
  }

  function getToolStatusLabel(status: ToolStreamItem['status']) {
    if (status === 'done') return '完成'
    if (status === 'error') return '失败'
    return '执行中'
  }

  function renderToolArguments(item: ToolStreamItem) {
    const parsed = parseJsonRecord(item.arguments)
    if (normalizeToolName(item.name) === 'workspace_run_command') {
      const command = parsed
        ? (typeof parsed.command === 'string' ? parsed.command.trim() : '')
        : extractJsonLikeStringField(item.arguments, 'command')
      const workdir = parsed
        ? (typeof parsed.workdir === 'string' ? parsed.workdir.trim() : '')
        : extractJsonLikeStringField(item.arguments, 'workdir')
      if (command) {
        return workdir ? `command: ${command}\nworkdir: ${workdir}` : `command: ${command}`
      }
    }
    return formatToolPayload(item.arguments)
  }

  function renderToolResult(item: ToolStreamItem) {
    return formatToolPayload(item.result)
  }

  function getDisplayedMessageContent(message: Message) {
    if (message.role !== 'assistant') return message.content

    const baseContent = assistantContentOverrides.value[message.id] ?? message.content
    const reasoningText = getReasoningEntry(message.id)?.text || ''
    return stripReasoningPrefixOverlap(baseContent, reasoningText)
  }

  function extractMessageToolCalls(message: Message) {
    return Array.isArray(message.tool_calls) ? message.tool_calls : []
  }

  function extractMessageToolMeta(message: Message) {
    const firstMeta = Array.isArray(message.tool_calls) ? message.tool_calls[0] : null
    const id = typeof firstMeta?.id === 'string' && firstMeta.id.trim() ? firstMeta.id.trim() : ''
    const name =
      (typeof firstMeta?.tool_name === 'string' && firstMeta.tool_name.trim()) ||
      (typeof firstMeta?.toolName === 'string' && firstMeta.toolName.trim()) ||
      ''
    return { id, name }
  }

  function normalizeReasoningText(value: unknown) {
    if (typeof value !== 'string') return ''
    return value.trim()
  }

  function mergeReasoningParts(parts: string[]) {
    let merged = ''
    for (const part of parts) {
      merged = mergeTextWithOverlap(merged, part)
    }
    return merged
  }

  function mergeTextWithOverlap(base: string, next: string) {
    const left = base.trim()
    const right = next.trim()
    if (!left) return right
    if (!right) return left
    if (left.includes(right)) return left
    if (right.includes(left)) return right

    const max = Math.min(left.length, right.length)
    let overlap = 0
    for (let len = max; len >= 10; len -= 1) {
      if (left.slice(left.length - len) === right.slice(0, len)) {
        overlap = len
        break
      }
    }

    if (overlap > 0) {
      return `${left}${right.slice(overlap)}`
    }
    return `${left}\n\n${right}`
  }

  function stripReasoningPrefixOverlap(content: string, reasoningText: string) {
    if (!content.trim() || !reasoningText.trim()) return content

    const source = content
    const trimmedSource = source.trimStart()
    const reasoning = reasoningText.trim()
    const max = Math.min(trimmedSource.length, reasoning.length)
    let overlap = 0

    for (let len = max; len >= 8; len -= 1) {
      if (reasoning.slice(reasoning.length - len) === trimmedSource.slice(0, len)) {
        overlap = len
        break
      }
    }

    if (overlap === 0) return content
    const leadingWhitespaceLength = source.length - trimmedSource.length
    return source.slice(0, leadingWhitespaceLength) + trimmedSource.slice(overlap).trimStart()
  }

  function getToolStats(items: ToolStreamItem[]) {
    const total = items.length
    const done = items.filter((item) => item.status === 'done').length
    const error = items.filter((item) => item.status === 'error').length
    const running = total - done - error
    return { total, done, error, running }
  }

  function renderToolStepName(name: string) {
    const normalized = normalizeToolName(name)
    if (!normalized) return '工具执行'
    if (options.toolStepLabels[normalized]) return options.toolStepLabels[normalized]
    return renderToolLabel(normalized)
  }

  function extractToolStepDetail(item: ToolStreamItem) {
    const normalized = normalizeToolName(item.name)
    const parsed = parseJsonRecord(item.arguments)

    if (normalized === 'workspace_run_command') {
      const command = parsed
        ? (typeof parsed.command === 'string' ? parsed.command.trim() : '')
        : extractJsonLikeStringField(item.arguments, 'command')
      return command ? truncateMiddle(command, 56) : ''
    }

    if (
      normalized === 'workspace_read_file' ||
      normalized === 'workspace_write_file' ||
      normalized === 'workspace_list_directory'
    ) {
      const path = parsed
        ? (typeof parsed.path === 'string' ? parsed.path.trim() : '')
        : extractJsonLikeStringField(item.arguments, 'path')
      return path ? truncateMiddle(path, 44) : ''
    }

    return ''
  }

  function serializeToolArguments(value: unknown) {
    if (typeof value === 'string') return value
    if (value === null || value === undefined) return ''
    try {
      return JSON.stringify(value, null, 2)
    } catch {
      return String(value)
    }
  }

  function inferToolResultStatus(result: string): ToolStreamItem['status'] {
    const parsed = parseJsonRecord(result)
    if (parsed && typeof parsed.error === 'string' && parsed.error.trim()) {
      return 'error'
    }
    return 'done'
  }

  return {
    reasoningByMessage,
    toolStreamItems,
    toolHistoryByMessage,
    toolListCollapsedByMessage,
    collapseActiveReasoning,
    getReasoningEntry,
    toggleReasoning,
    isStreamingMessage,
    isRenderableMessage,
    hydrateConversationArtifacts,
    shouldShowMessageBubble,
    shouldRenderAssistantContent,
    persistToolItemsForActiveMessage,
    initializeAssistantArtifacts,
    clearAssistantArtifacts,
    getToolItemsForMessage,
    shouldShowToolProgress,
    isToolListCollapsed,
    toggleCompactToolList,
    getCompactToolSummaryTitle,
    getCompactToolSummaryCount,
    renderToolItemName,
    getToolStatusLabel,
    renderToolArguments,
    renderToolResult,
    getDisplayedMessageContent
  }
}
