import { listen } from '@tauri-apps/api/event'

export interface ToolApprovalRequest {
  requestId: string
  conversationId: string
  toolCallId: string
  toolName: string
  arguments: string
}

export type TimelineEventType =
  | 'user_message'
  | 'assistant_reasoning'
  | 'assistant_text'
  | 'assistant_tool_call'
  | 'assistant_tool_result'

export interface TimelineEventInput {
  conversation_id: string
  turn_id: string
  seq: number
  event_type: TimelineEventType
  tool_call_id?: string | null
  payload: Record<string, unknown>
  created_at?: string
}

interface ChatStoreBridge {
  currentConversationId: string | null
  setConversationStreaming: (conversationId: string, isStreaming: boolean) => void
  appendTimelineEvent: (event: TimelineEventInput) => void
}

interface ChatEventBridgeOptions {
  chatStore: ChatStoreBridge
  onToolApprovalRequest?: (request: ToolApprovalRequest) => void
  onStreamEnd: (conversationId: string | null) => void
}

function asObjectPayload(value: unknown): Record<string, unknown> | null {
  if (!value || typeof value !== 'object') return null
  return value as Record<string, unknown>
}

function readString(record: Record<string, unknown>, key: string) {
  const value = record[key]
  return typeof value === 'string' ? value : ''
}

function readNumber(record: Record<string, unknown>, key: string) {
  const value = record[key]
  if (typeof value === 'number' && Number.isFinite(value)) return value
  return 0
}

function readOptionalString(record: Record<string, unknown>, key: string) {
  const value = record[key]
  return typeof value === 'string' && value.trim() ? value : null
}

function buildTimelineEvent(
  payload: Record<string, unknown>,
  fallbackConversationId: string | null,
  fallbackEventType: TimelineEventType,
  fallbackData: Record<string, unknown>
): TimelineEventInput | null {
  const conversationId = readString(payload, 'conversationId') || fallbackConversationId || ''
  if (!conversationId) return null
  const turnId = readString(payload, 'turnId') || 'legacy-stream-turn'
  const seq = readNumber(payload, 'seq')
  const eventType = (readString(payload, 'eventType') as TimelineEventType) || fallbackEventType
  const createdAt = readString(payload, 'createdAt') || new Date().toISOString()
  const toolCallId = readOptionalString(payload, 'toolCallId')
  return {
    conversation_id: conversationId,
    turn_id: turnId,
    seq,
    event_type: eventType,
    tool_call_id: toolCallId,
    payload: fallbackData,
    created_at: createdAt
  }
}

export async function registerChatEventListeners(options: ChatEventBridgeOptions) {
  const unlistenFns: Array<() => void> = []

  unlistenFns.push(
    await listen('chat-user-message', (event) => {
      const payload = asObjectPayload(event.payload)
      if (!payload) return
      const timelineEvent = buildTimelineEvent(payload, options.chatStore.currentConversationId, 'user_message', {
        content: readString(payload, 'content')
      })
      if (!timelineEvent) return
      options.chatStore.appendTimelineEvent(timelineEvent)
    })
  )

  unlistenFns.push(
    await listen('chat-chunk', (event) => {
      const payload = asObjectPayload(event.payload)
      if (!payload) return
      const timelineEvent = buildTimelineEvent(payload, options.chatStore.currentConversationId, 'assistant_text', {
        text: readString(payload, 'chunk')
      })
      if (!timelineEvent) return
      options.chatStore.appendTimelineEvent(timelineEvent)
    })
  )

  unlistenFns.push(
    await listen('chat-reasoning', (event) => {
      const payload = asObjectPayload(event.payload)
      if (!payload) return
      const timelineEvent = buildTimelineEvent(payload, options.chatStore.currentConversationId, 'assistant_reasoning', {
        text: readString(payload, 'chunk')
      })
      if (!timelineEvent) return
      options.chatStore.appendTimelineEvent(timelineEvent)
    })
  )

  unlistenFns.push(
    await listen('chat-tool-call', (event) => {
      const payload = asObjectPayload(event.payload)
      if (!payload) return
      const timelineEvent = buildTimelineEvent(payload, options.chatStore.currentConversationId, 'assistant_tool_call', {
        index: payload.index,
        name: payload.name,
        argumentsChunk: payload.argumentsChunk
      })
      if (!timelineEvent) return
      options.chatStore.appendTimelineEvent(timelineEvent)
    })
  )

  unlistenFns.push(
    await listen('chat-tool-result', (event) => {
      const payload = asObjectPayload(event.payload)
      if (!payload) return
      const timelineEvent = buildTimelineEvent(payload, options.chatStore.currentConversationId, 'assistant_tool_result', {
        name: payload.name,
        result: payload.result,
        error: payload.error
      })
      if (!timelineEvent) return
      options.chatStore.appendTimelineEvent(timelineEvent)
    })
  )

  unlistenFns.push(
    await listen('chat-end', (event) => {
      let conversationId: string | null = options.chatStore.currentConversationId
      const payload = asObjectPayload(event.payload)
      if (payload) {
        const value = readString(payload, 'conversationId')
        if (value) conversationId = value
      }

      if (conversationId) {
        options.chatStore.setConversationStreaming(conversationId, false)
      }
      options.onStreamEnd(conversationId)
    })
  )

  unlistenFns.push(
    await listen('chat-tool-approval-request', (event) => {
      const payload = asObjectPayload(event.payload)
      if (!payload || !options.onToolApprovalRequest) return

      const requestId = readString(payload, 'requestId')
      const conversationId = readString(payload, 'conversationId')
      const toolCallId = readString(payload, 'toolCallId')
      const toolName = readString(payload, 'toolName')
      if (!requestId || !conversationId || !toolCallId || !toolName) return

      options.onToolApprovalRequest({
        requestId,
        conversationId,
        toolCallId,
        toolName,
        arguments: readString(payload, 'arguments')
      })
    })
  )

  return unlistenFns
}
