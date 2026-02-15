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
  index?: number
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
  setConversationStreaming: (conversationId: string, isStreaming: boolean) => void
}

interface ChatEventBridgeOptions {
  chatStore: ChatStoreBridge
  activeAssistantMessageId: Ref<string | null>
  reasoningByMessage: Ref<Record<string, ReasoningEntry>>
  toolStreamItems: Ref<ToolStreamItem[]>
  onToolApprovalRequest?: (request: ToolApprovalRequest) => void
  onStreamEnd: (conversationId: string | null) => void
}

export async function registerChatEventListeners(options: ChatEventBridgeOptions) {
  const unlistenFns: Array<() => void> = []
  let anonymousToolCounter = 0

  function appendWithOverlap(base: string, chunk: string) {
    if (!base) return chunk
    if (!chunk) return base
    if (base.includes(chunk)) return base
    if (chunk.startsWith(base)) return chunk

    const max = Math.min(base.length, chunk.length)
    for (let len = max; len >= 6; len -= 1) {
      if (base.slice(base.length - len) === chunk.slice(0, len)) {
        return base + chunk.slice(len)
      }
    }
    return base + chunk
  }

  function provisionalToolId(index?: number) {
    anonymousToolCounter += 1
    if (typeof index === 'number' && Number.isFinite(index)) {
      return `tool-${index}-${anonymousToolCounter}`
    }
    return `tool-anon-${anonymousToolCounter}`
  }

  function findToolItem(
    id?: string,
    index?: number,
    name?: string,
    preferRunning = false
  ) {
    const items = options.toolStreamItems.value
    if (id) {
      const byId = items.find((entry) => entry.id === id)
      if (byId) return byId
    }

    if (typeof index === 'number' && Number.isFinite(index)) {
      const byIndex = items.find(
        (entry) => entry.index === index && (!preferRunning || entry.status === 'running')
      )
      if (byIndex) return byIndex
    }

    if (name) {
      const byName = items.find((entry) => entry.name === name && (!preferRunning || entry.status === 'running'))
      if (byName) return byName
    }

    if (preferRunning) {
      return items.find((entry) => entry.status === 'running')
    }

    return undefined
  }

  function mergeToolItem(target: ToolStreamItem, source: ToolStreamItem) {
    if (!target.arguments && source.arguments) {
      target.arguments = source.arguments
    }
    if (!target.result && source.result) {
      target.result = source.result
    }
    if (target.status === 'running' && source.status !== 'running') {
      target.status = source.status
    }
    if (typeof target.index !== 'number' && typeof source.index === 'number') {
      target.index = source.index
    }
  }

  function normalizeToolOrder() {
    options.toolStreamItems.value.sort((a, b) => {
      const left = typeof a.index === 'number' ? a.index : Number.MAX_SAFE_INTEGER
      const right = typeof b.index === 'number' ? b.index : Number.MAX_SAFE_INTEGER
      return left - right
    })
  }

  unlistenFns.push(
    await listen('chat-chunk', (event) => {
      let chunk = ''
      let conversationId: string | null = options.chatStore.currentConversationId
      if (typeof event.payload === 'string') {
        chunk = event.payload
      } else if (event.payload && typeof event.payload === 'object') {
        const payload = event.payload as { conversationId?: string; chunk?: string }
        chunk = typeof payload.chunk === 'string' ? payload.chunk : ''
        conversationId = typeof payload.conversationId === 'string' ? payload.conversationId : conversationId
      }

      if (!conversationId || !chunk) return
      options.chatStore.updateLastMessage(conversationId, chunk)
    })
  )

  unlistenFns.push(
    await listen('chat-end', (event) => {
      let conversationId: string | null = options.chatStore.currentConversationId
      if (event.payload && typeof event.payload === 'object') {
        const payload = event.payload as { conversationId?: string }
        if (typeof payload.conversationId === 'string') {
          conversationId = payload.conversationId
        }
      }

      if (conversationId) {
        options.chatStore.setConversationStreaming(conversationId, false)
      }
      options.onStreamEnd(conversationId)
      if (conversationId && conversationId === options.chatStore.currentConversationId) {
        options.activeAssistantMessageId.value = null
      }
    })
  )

  unlistenFns.push(
    await listen('chat-reasoning', (event) => {
      let chunk = ''
      let conversationId: string | null = options.chatStore.currentConversationId
      if (typeof event.payload === 'string') {
        chunk = event.payload
      } else if (event.payload && typeof event.payload === 'object') {
        const payload = event.payload as { conversationId?: string; chunk?: string }
        chunk = typeof payload.chunk === 'string' ? payload.chunk : ''
        conversationId = typeof payload.conversationId === 'string' ? payload.conversationId : conversationId
      }

      if (conversationId !== options.chatStore.currentConversationId) return
      if (!chunk || !options.activeAssistantMessageId.value) return

      const id = options.activeAssistantMessageId.value
      if (!options.reasoningByMessage.value[id]) {
        options.reasoningByMessage.value[id] = { text: '', collapsed: false }
      }
      options.reasoningByMessage.value[id].text = appendWithOverlap(
        options.reasoningByMessage.value[id].text,
        chunk
      )
      options.reasoningByMessage.value[id].collapsed = false
    })
  )

  unlistenFns.push(
    await listen('chat-tool-call', (event) => {
      const payload = event.payload as {
        conversationId?: string
        index?: number
        toolCallId?: string
        name?: string
        argumentsChunk?: string
      }
      if (
        payload.conversationId &&
        payload.conversationId !== options.chatStore.currentConversationId
      ) {
        return
      }
      const id = payload.toolCallId || provisionalToolId(payload.index)
      let item = payload.toolCallId
        ? findToolItem(payload.toolCallId, payload.index, payload.name)
        : findToolItem(undefined, payload.index, payload.name, true)

      if (!item) {
        item = { id, name: payload.name || 'tool', arguments: '', result: '', status: 'running', index: payload.index }
        options.toolStreamItems.value.push(item)
      } else if (!payload.toolCallId && item.status !== 'running') {
        item = { id, name: payload.name || 'tool', arguments: '', result: '', status: 'running', index: payload.index }
        options.toolStreamItems.value.push(item)
      } else if (payload.toolCallId && item.id !== payload.toolCallId) {
        const duplicate = options.toolStreamItems.value.find((entry) => entry.id === payload.toolCallId)
        if (duplicate && duplicate !== item) {
          mergeToolItem(duplicate, item)
          const removeIndex = options.toolStreamItems.value.indexOf(item)
          if (removeIndex >= 0) {
            options.toolStreamItems.value.splice(removeIndex, 1)
          }
          item = duplicate
        } else {
          item.id = payload.toolCallId
        }
      }

      if (payload.name) item.name = payload.name
      if (typeof payload.index === 'number' && Number.isFinite(payload.index)) {
        item.index = payload.index
      }
      if (payload.argumentsChunk) item.arguments += payload.argumentsChunk
      normalizeToolOrder()
    })
  )

  unlistenFns.push(
    await listen('chat-tool-result', (event) => {
      const payload = event.payload as {
        conversationId?: string
        toolCallId?: string
        name?: string
        result?: string | null
        error?: string | null
      }
      if (
        payload.conversationId &&
        payload.conversationId !== options.chatStore.currentConversationId
      ) {
        return
      }
      const id = payload.toolCallId || provisionalToolId(undefined)
      let item = findToolItem(payload.toolCallId, undefined, payload.name, true)

      if (!item) {
        item = { id, name: payload.name || 'tool', arguments: '', result: '', status: 'running' }
        options.toolStreamItems.value.push(item)
      } else if (payload.toolCallId && item.id !== payload.toolCallId) {
        const duplicate = options.toolStreamItems.value.find((entry) => entry.id === payload.toolCallId)
        if (duplicate && duplicate !== item) {
          mergeToolItem(duplicate, item)
          const removeIndex = options.toolStreamItems.value.indexOf(item)
          if (removeIndex >= 0) {
            options.toolStreamItems.value.splice(removeIndex, 1)
          }
          item = duplicate
        } else {
          item.id = payload.toolCallId
        }
      }

      if (payload.name) item.name = payload.name
      item.status = payload.error ? 'error' : 'done'
      item.result = payload.error || payload.result || ''
      normalizeToolOrder()
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
