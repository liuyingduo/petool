import { defineStore } from 'pinia'
import { ref, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'

export interface Message {
  id: string
  conversation_id: string
  role: 'user' | 'assistant' | 'system' | 'tool'
  content: string
  reasoning?: string | null
  created_at: string
  tool_calls?: any[]
}

export interface Conversation {
  id: string
  title: string
  model: string
  created_at: string
  updated_at: string
}

export type TimelineEventType =
  | 'user_message'
  | 'assistant_reasoning'
  | 'assistant_text'
  | 'assistant_tool_call'
  | 'assistant_tool_result'

export interface TimelineEvent {
  id: string
  conversation_id: string
  turn_id: string
  seq: number
  event_type: TimelineEventType
  tool_call_id?: string | null
  payload: Record<string, unknown>
  created_at: string
}

interface ConversationTimelineResponse {
  events: TimelineEvent[]
  legacy: boolean
}

interface TimelineEventInput {
  id?: string
  conversation_id: string
  turn_id: string
  seq: number
  event_type: TimelineEventType
  tool_call_id?: string | null
  payload: Record<string, unknown>
  created_at?: string
}

export const useChatStore = defineStore('chat', () => {
  const conversations = ref<Conversation[]>([])
  const messages = ref<Record<string, Message[]>>({})
  const currentConversationId = ref<string | null>(null)
  const loading = ref(false)
  const streaming = ref(false)
  const streamingByConversation = ref<Record<string, boolean>>({})
  const timelineByConversation = ref<Record<string, TimelineEvent[]>>({})
  const timelineLegacyByConversation = ref<Record<string, boolean>>({})

  const currentMessages = computed(() => {
    if (!currentConversationId.value) return []
    return messages.value[currentConversationId.value] || []
  })

  const currentConversation = computed(() => {
    if (!currentConversationId.value) return null
    return conversations.value.find(c => c.id === currentConversationId.value) || null
  })

  const currentTimeline = computed(() => {
    if (!currentConversationId.value) return []
    return timelineByConversation.value[currentConversationId.value] || []
  })

  const currentTimelineLegacy = computed(() => {
    if (!currentConversationId.value) return false
    return Boolean(timelineLegacyByConversation.value[currentConversationId.value])
  })

  async function loadConversations() {
    loading.value = true
    try {
      conversations.value = await invoke<Conversation[]>('get_conversations')
    } catch (error) {
      console.error('Failed to load conversations:', error)
    } finally {
      loading.value = false
    }
  }

  async function loadMessages(conversationId: string) {
    loading.value = true
    try {
      const msgs = await invoke<Message[]>('get_messages', { conversationId })
      messages.value[conversationId] = msgs
    } catch (error) {
      console.error('Failed to load messages:', error)
    } finally {
      loading.value = false
    }
  }

  async function loadTimeline(conversationId: string) {
    loading.value = true
    try {
      const result = await invoke<ConversationTimelineResponse>('get_conversation_timeline', { conversationId })
      timelineByConversation.value[conversationId] = normalizeTimelineEvents(result.events || [])
      timelineLegacyByConversation.value[conversationId] = Boolean(result.legacy)
    } catch (error) {
      console.error('Failed to load timeline:', error)
      timelineByConversation.value[conversationId] = []
      timelineLegacyByConversation.value[conversationId] = false
    } finally {
      loading.value = false
    }
  }

  async function createConversation(title: string, model: string) {
    try {
      const conversation = await invoke<Conversation>('create_conversation', { title, model })
      conversations.value.unshift(conversation)
      return conversation
    } catch (error) {
      console.error('Failed to create conversation:', error)
      throw error
    }
  }

  async function deleteConversation(id: string) {
    try {
      await invoke('delete_conversation', { id })
      conversations.value = conversations.value.filter(c => c.id !== id)
      delete messages.value[id]
      delete timelineByConversation.value[id]
      delete timelineLegacyByConversation.value[id]
      delete streamingByConversation.value[id]
      streaming.value = Object.values(streamingByConversation.value).some(Boolean)
      if (currentConversationId.value === id) {
        currentConversationId.value = null
      }
    } catch (error) {
      console.error('Failed to delete conversation:', error)
      throw error
    }
  }

  async function renameConversation(id: string, title: string) {
    try {
      await invoke('rename_conversation', { id, title })
      const now = new Date().toISOString()
      conversations.value = conversations.value.map((conversation) =>
        conversation.id === id
          ? {
              ...conversation,
              title,
              updated_at: now
            }
          : conversation
      )
    } catch (error) {
      console.error('Failed to rename conversation:', error)
      throw error
    }
  }

  async function updateConversationModel(id: string, model: string) {
    try {
      await invoke('update_conversation_model', { id, model })
      const now = new Date().toISOString()
      conversations.value = conversations.value.map((conversation) =>
        conversation.id === id
          ? {
              ...conversation,
              model,
              updated_at: now
            }
          : conversation
      )
    } catch (error) {
      console.error('Failed to update conversation model:', error)
      throw error
    }
  }

  function setCurrentConversation(id: string | null) {
    currentConversationId.value = id
  }

  function addMessage(conversationId: string, message: Message) {
    if (!messages.value[conversationId]) {
      messages.value[conversationId] = []
    }
    messages.value[conversationId].push(message)
  }

  function updateLastMessage(conversationId: string, chunk: string) {
    const msgs = messages.value[conversationId]
    if (msgs && msgs.length > 0) {
      const last = msgs[msgs.length - 1]
      if (last.role === 'assistant') {
        if (!chunk) return
        last.content += chunk
      }
    }
  }

  function setConversationStreaming(conversationId: string, isStreaming: boolean) {
    if (!conversationId) return
    if (isStreaming) {
      streamingByConversation.value[conversationId] = true
    } else {
      delete streamingByConversation.value[conversationId]
    }
    streaming.value = Object.values(streamingByConversation.value).some(Boolean)
  }

  function isConversationStreaming(conversationId: string | null | undefined) {
    if (!conversationId) return false
    return Boolean(streamingByConversation.value[conversationId])
  }

  function appendTimelineEvent(event: TimelineEventInput) {
    const conversationId = event.conversation_id
    if (!timelineByConversation.value[conversationId]) {
      timelineByConversation.value[conversationId] = []
    }
    timelineLegacyByConversation.value[conversationId] = false

    const normalizedEvent: TimelineEvent = {
      id: event.id || `${event.turn_id}-${event.seq}-${Date.now()}`,
      conversation_id: conversationId,
      turn_id: event.turn_id,
      seq: event.seq,
      event_type: event.event_type,
      tool_call_id: event.tool_call_id || null,
      payload: event.payload || {},
      created_at: event.created_at || new Date().toISOString()
    }

    const target = timelineByConversation.value[conversationId]
    if (target.length === 0) {
      target.push(normalizedEvent)
      return
    }

    const last = target[target.length - 1]
    if (tryMergeTimelineEvent(last, normalizedEvent)) {
      return
    }

    if (compareTimelineOrder(last, normalizedEvent) <= 0) {
      target.push(normalizedEvent)
      return
    }

    const insertIndex = target.findIndex((item) => compareTimelineOrder(normalizedEvent, item) < 0)
    if (insertIndex < 0) {
      target.push(normalizedEvent)
    } else {
      target.splice(insertIndex, 0, normalizedEvent)
    }
  }

  function normalizeTimelineEvents(events: TimelineEvent[]) {
    const sorted = [...events].sort((a, b) => {
      return compareTimelineOrder(a, b)
    })

    const merged: TimelineEvent[] = []
    for (const event of sorted) {
      const normalized: TimelineEvent = {
        ...event,
        payload: event.payload || {}
      }
      const last = merged[merged.length - 1]
      if (last && tryMergeTimelineEvent(last, normalized)) {
        continue
      }
      merged.push(normalized)
    }
    return merged
  }

  function tryMergeTimelineEvent(base: TimelineEvent, next: TimelineEvent) {
    if (base.turn_id !== next.turn_id) return false
    if (base.event_type !== next.event_type) return false

    if (next.event_type === 'assistant_text' || next.event_type === 'assistant_reasoning') {
      const baseText = typeof base.payload.text === 'string' ? base.payload.text : ''
      const nextText = typeof next.payload.text === 'string' ? next.payload.text : ''
      base.payload = { ...base.payload, text: mergeTextChunk(baseText, nextText) }
      base.seq = next.seq
      base.created_at = next.created_at
      return true
    }

    if (next.event_type === 'assistant_tool_call') {
      const baseIndex = readToolCallIndex(base)
      const nextIndex = readToolCallIndex(next)
      const sameIndex = baseIndex !== null && nextIndex !== null && baseIndex === nextIndex

      if (base.tool_call_id && next.tool_call_id && base.tool_call_id !== next.tool_call_id) {
        return false
      }
      if (!base.tool_call_id || !next.tool_call_id) {
        if (!sameIndex) return false
      }

      if (!base.tool_call_id && next.tool_call_id) {
        base.tool_call_id = next.tool_call_id
      }

      const mergedIndex = nextIndex ?? baseIndex
      const baseName = readToolCallName(base)
      const nextName = readToolCallName(next)
      const mergedName = nextName || baseName

      const basePayload = base.payload || {}
      const nextPayload = next.payload || {}
      const baseArgs = typeof basePayload.argumentsChunk === 'string' ? basePayload.argumentsChunk : ''
      const nextArgs = typeof nextPayload.argumentsChunk === 'string' ? nextPayload.argumentsChunk : ''
      base.payload = {
        ...basePayload,
        ...nextPayload,
        ...(mergedIndex !== null ? { index: mergedIndex } : {}),
        ...(mergedName ? { name: mergedName } : {}),
        argumentsChunk: mergeToolArgumentsChunk(baseArgs, nextArgs)
      }
      base.seq = next.seq
      base.created_at = next.created_at
      return true
    }

    return false
  }

  function readToolCallIndex(event: TimelineEvent) {
    const payload = event.payload || {}
    const raw = payload.index
    if (typeof raw === 'number' && Number.isFinite(raw)) return raw
    return null
  }

  function readToolCallName(event: TimelineEvent) {
    const payload = event.payload || {}
    const raw = payload.name
    if (typeof raw === 'string') {
      const trimmed = raw.trim()
      if (trimmed) return trimmed
    }
    return ''
  }

  function compareTimelineOrder(left: TimelineEvent, right: TimelineEvent) {
    const leftTimeRaw = Date.parse(left.created_at)
    const rightTimeRaw = Date.parse(right.created_at)
    const leftTime = Number.isFinite(leftTimeRaw) ? leftTimeRaw : 0
    const rightTime = Number.isFinite(rightTimeRaw) ? rightTimeRaw : 0
    if (leftTime !== rightTime) return leftTime - rightTime
    if (left.turn_id === right.turn_id && left.seq !== right.seq) {
      return left.seq - right.seq
    }
    return 0
  }

  function mergeTextChunk(base: string, chunk: string) {
    if (!base) return chunk
    if (!chunk) return base
    if (chunk === base) return base
    if (chunk.startsWith(base)) return chunk
    return base + chunk
  }

  function mergeToolArgumentsChunk(base: string, chunk: string) {
    if (!base) return chunk
    if (!chunk) return base
    if (chunk === base) return base
    if (chunk.startsWith(base)) return chunk

    const max = Math.min(base.length, chunk.length)
    for (let overlap = max - 1; overlap >= 1; overlap -= 1) {
      if (base.slice(base.length - overlap) === chunk.slice(0, overlap)) {
        return base + chunk.slice(overlap)
      }
    }
    return base + chunk
  }

  return {
    conversations,
    messages,
    timelineByConversation,
    timelineLegacyByConversation,
    currentConversationId,
    currentMessages,
    currentConversation,
    currentTimeline,
    currentTimelineLegacy,
    loading,
    streaming,
    streamingByConversation,
    loadConversations,
    loadMessages,
    loadTimeline,
    createConversation,
    deleteConversation,
    renameConversation,
    updateConversationModel,
    setCurrentConversation,
    addMessage,
    updateLastMessage,
    setConversationStreaming,
    isConversationStreaming,
    appendTimelineEvent
  }
})
