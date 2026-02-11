import { listen } from '@tauri-apps/api/event'
import type { Ref } from 'vue'

export interface ReasoningEntry {
  text: string
  collapsed: boolean
}

export interface ToolStreamItem {
  id: string
  name: string
  arguments: string
  result: string
  status: 'running' | 'done' | 'error'
}

export interface ToolApprovalRequest {
  requestId: string
  conversationId: string
  toolCallId: string
  toolName: string
  arguments: string
}

interface ChatStoreBridge {
  currentConversationId: string | null
  streaming: boolean
  updateLastMessage: (conversationId: string, chunk: string) => void
}

interface ChatEventBridgeOptions {
  chatStore: ChatStoreBridge
  activeAssistantMessageId: Ref<string | null>
  reasoningByMessage: Ref<Record<string, ReasoningEntry>>
  toolStreamItems: Ref<ToolStreamItem[]>
  onToolApprovalRequest?: (request: ToolApprovalRequest) => void
  onStreamEnd: () => void
}

export async function registerChatEventListeners(options: ChatEventBridgeOptions) {
  const unlistenFns: Array<() => void> = []

  unlistenFns.push(
    await listen('chat-chunk', (event) => {
      const chunk = event.payload as string
      if (!options.chatStore.currentConversationId) return
      options.chatStore.updateLastMessage(options.chatStore.currentConversationId, chunk)
    })
  )

  unlistenFns.push(
    await listen('chat-end', () => {
      options.chatStore.streaming = false
      options.onStreamEnd()
      options.activeAssistantMessageId.value = null
      options.toolStreamItems.value = []
    })
  )

  unlistenFns.push(
    await listen('chat-reasoning', (event) => {
      const chunk = event.payload as string
      if (!chunk || !options.activeAssistantMessageId.value) return

      const id = options.activeAssistantMessageId.value
      if (!options.reasoningByMessage.value[id]) {
        options.reasoningByMessage.value[id] = { text: '', collapsed: false }
      }
      options.reasoningByMessage.value[id].text += chunk
      options.reasoningByMessage.value[id].collapsed = false
    })
  )

  unlistenFns.push(
    await listen('chat-tool-call', (event) => {
      const payload = event.payload as { index?: number; toolCallId?: string; name?: string; argumentsChunk?: string }
      const id = payload.toolCallId || `tool-${payload.index ?? 0}`
      let item = options.toolStreamItems.value.find((entry) => entry.id === id)

      if (!item) {
        item = { id, name: payload.name || 'tool', arguments: '', result: '', status: 'running' }
        options.toolStreamItems.value.push(item)
      }

      if (payload.name) item.name = payload.name
      if (payload.argumentsChunk) item.arguments += payload.argumentsChunk
    })
  )

  unlistenFns.push(
    await listen('chat-tool-result', (event) => {
      const payload = event.payload as { toolCallId?: string; name?: string; result?: string | null; error?: string | null }
      const id = payload.toolCallId || `tool-${Date.now()}`
      let item = options.toolStreamItems.value.find((entry) => entry.id === id)

      if (!item) {
        item = { id, name: payload.name || 'tool', arguments: '', result: '', status: 'running' }
        options.toolStreamItems.value.push(item)
      }

      if (payload.name) item.name = payload.name
      item.status = payload.error ? 'error' : 'done'
      item.result = payload.error || payload.result || ''
    })
  )

  unlistenFns.push(
    await listen('chat-tool-approval-request', (event) => {
      const payload = event.payload as Partial<ToolApprovalRequest>
      if (!options.onToolApprovalRequest) return
      if (!payload.requestId || !payload.conversationId || !payload.toolCallId || !payload.toolName) return

      options.onToolApprovalRequest({
        requestId: payload.requestId,
        conversationId: payload.conversationId,
        toolCallId: payload.toolCallId,
        toolName: payload.toolName,
        arguments: payload.arguments || ''
      })
    })
  )

  return unlistenFns
}
