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

export const useChatStore = defineStore('chat', () => {
  const conversations = ref<Conversation[]>([])
  const messages = ref<Record<string, Message[]>>({})
  const currentConversationId = ref<string | null>(null)
  const loading = ref(false)
  const streaming = ref(false)
  const streamingByConversation = ref<Record<string, boolean>>({})

  const currentMessages = computed(() => {
    if (!currentConversationId.value) return []
    return messages.value[currentConversationId.value] || []
  })

  const currentConversation = computed(() => {
    if (!currentConversationId.value) return null
    return conversations.value.find(c => c.id === currentConversationId.value) || null
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

  return {
    conversations,
    messages,
    currentConversationId,
    currentMessages,
    currentConversation,
    loading,
    streaming,
    streamingByConversation,
    loadConversations,
    loadMessages,
    createConversation,
    deleteConversation,
    renameConversation,
    updateConversationModel,
    setCurrentConversation,
    addMessage,
    updateLastMessage,
    setConversationStreaming,
    isConversationStreaming
  }
})
