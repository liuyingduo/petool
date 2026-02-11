<template>
  <div class="chat-panel-container">
    <!-- Header -->
    <div class="chat-header" v-if="chatStore.currentConversation">
      <div class="chat-title">
        <el-icon><ChatDotRound /></el-icon>
        {{ chatStore.currentConversation.title }}
      </div>
      <div class="chat-model">{{ chatStore.currentConversation.model }}</div>
    </div>

    <!-- Empty State -->
    <div v-else class="chat-empty">
      <el-icon :size="64" color="var(--color-text-secondary)"><ChatDotRound /></el-icon>
      <p class="empty-text">Select or create a conversation to start chatting</p>
    </div>

    <!-- Messages -->
    <div v-if="chatStore.currentConversationId" class="messages-container">
      <div
        v-for="message in chatStore.currentMessages"
        :key="message.id"
        class="message"
        :class="message.role"
      >
        <div class="message-avatar">
          <el-icon v-if="message.role === 'user'"><User /></el-icon>
          <el-icon v-else><Cpu /></el-icon>
        </div>
        <div class="message-content">
          <div class="message-text" v-html="renderMarkdown(message.content)"></div>
        </div>
      </div>

      <!-- Loading indicator -->
      <div v-if="chatStore.streaming" class="message assistant">
        <div class="message-avatar">
          <el-icon><Cpu /></el-icon>
        </div>
        <div class="message-content">
          <div v-if="reasoningStream" class="reasoning-stream">
            {{ reasoningStream }}
          </div>

          <div v-if="toolStreamItems.length > 0" class="tool-stream-list">
            <div
              v-for="item in toolStreamItems"
              :key="item.id"
              class="tool-stream-card"
              :class="item.status"
            >
              <div class="tool-title">{{ item.name || 'tool' }}</div>
              <div v-if="item.arguments" class="tool-args">{{ item.arguments }}</div>
              <div v-if="item.result" class="tool-result">{{ item.result }}</div>
            </div>
          </div>

          <div class="typing-indicator">
            <span></span>
            <span></span>
            <span></span>
          </div>
        </div>
      </div>
    </div>

    <!-- Input Area -->
    <div v-if="chatStore.currentConversationId" class="input-container">
      <div class="input-wrapper">
        <el-input
          v-model="inputMessage"
          type="textarea"
          :rows="1"
          :autosize="{ minRows: 1, maxRows: 8 }"
          placeholder="Type a message... (Ctrl+Enter to send)"
          @keydown.ctrl.enter="sendMessage"
        />
        <el-button
          type="primary"
          circle
          :loading="chatStore.streaming"
          @click="sendMessage"
        >
          <el-icon><Promotion /></el-icon>
        </el-button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { useChatStore } from '@/stores/chat'
import { ChatDotRound, User, Cpu, Promotion } from '@element-plus/icons-vue'
import { marked } from 'marked'
import { ElMessage } from 'element-plus'
import { listen } from '@tauri-apps/api/event'
import { invoke } from '@tauri-apps/api/core'

const chatStore = useChatStore()
const inputMessage = ref('')
const reasoningStream = ref('')
const toolStreamItems = ref<Array<{
  id: string
  name: string
  arguments: string
  result: string
  status: 'running' | 'done' | 'error'
}>>([])

// Listen for streaming events
listen('chat-chunk', (event) => {
  const chunk = event.payload as string
  chatStore.updateLastMessage(chatStore.currentConversationId!, chunk)
}).catch((error) => {
  console.error('Failed to subscribe chat-chunk:', error)
})

listen('chat-end', () => {
  const conversationId = chatStore.currentConversationId
  chatStore.streaming = false
  reasoningStream.value = ''
  toolStreamItems.value = []
  if (conversationId) {
    void chatStore.loadMessages(conversationId)
  }
}).catch((error) => {
  console.error('Failed to subscribe chat-end:', error)
})

listen('chat-reasoning', (event) => {
  const chunk = event.payload as string
  if (!chunk) return
  reasoningStream.value += chunk
}).catch((error) => {
  console.error('Failed to subscribe chat-reasoning:', error)
})

listen('chat-tool-call', (event) => {
  const payload = event.payload as {
    toolCallId?: string
    name?: string
    argumentsChunk?: string
  }

  const id = payload.toolCallId || `tool-${Date.now()}`
  const existing = toolStreamItems.value.find((item) => item.id === id)
  if (!existing) {
    toolStreamItems.value.push({
      id,
      name: payload.name || 'tool',
      arguments: payload.argumentsChunk || '',
      result: '',
      status: 'running'
    })
    return
  }

  if (payload.name) {
    existing.name = payload.name
  }
  if (payload.argumentsChunk) {
    existing.arguments += payload.argumentsChunk
  }
}).catch((error) => {
  console.error('Failed to subscribe chat-tool-call:', error)
})

listen('chat-tool-result', (event) => {
  const payload = event.payload as {
    toolCallId?: string
    name?: string
    result?: string | null
    error?: string | null
  }

  const id = payload.toolCallId || `tool-${Date.now()}`
  let item = toolStreamItems.value.find((entry) => entry.id === id)
  if (!item) {
    item = {
      id,
      name: payload.name || 'tool',
      arguments: '',
      result: '',
      status: 'running'
    }
    toolStreamItems.value.push(item)
  }

  if (payload.name) {
    item.name = payload.name
  }
  item.status = payload.error ? 'error' : 'done'
  item.result = payload.error ? payload.error : (payload.result || '')
}).catch((error) => {
  console.error('Failed to subscribe chat-tool-result:', error)
})

function renderMarkdown(content: string) {
  return marked(content)
}

function getErrorMessage(error: unknown): string {
  if (typeof error === 'string') return error
  if (error instanceof Error) return error.message
  if (error && typeof error === 'object' && 'message' in error) {
    const message = (error as { message?: unknown }).message
    if (typeof message === 'string' && message.trim().length > 0) {
      return message
    }
  }
  try {
    return JSON.stringify(error)
  } catch {
    return 'Failed to send message'
  }
}

async function sendMessage() {
  const content = inputMessage.value.trim()
  if (!content || chatStore.streaming) return

  inputMessage.value = ''
  reasoningStream.value = ''
  toolStreamItems.value = []

  // Add user message
  const userMsg: Message = {
    id: Date.now().toString(),
    conversation_id: chatStore.currentConversationId!,
    role: 'user',
    content,
    created_at: new Date().toISOString()
  }
  chatStore.addMessage(chatStore.currentConversationId!, userMsg)

  // Add placeholder assistant message
  const assistantMsg: Message = {
    id: (Date.now() + 1).toString(),
    conversation_id: chatStore.currentConversationId!,
    role: 'assistant',
    content: '',
    created_at: new Date().toISOString()
  }
  chatStore.addMessage(chatStore.currentConversationId!, assistantMsg)

  chatStore.streaming = true

  try {
    await invoke('stream_message', {
      conversationId: chatStore.currentConversationId,
      content,
    })
  } catch (error) {
    chatStore.streaming = false
    console.error('stream_message failed:', error)
    ElMessage.error(getErrorMessage(error))
  }
}

interface Message {
  id: string
  conversation_id: string
  role: 'user' | 'assistant' | 'system' | 'tool'
  content: string
  created_at: string
}
</script>

<style scoped>
.chat-panel-container {
  display: flex;
  flex-direction: column;
  height: 100%;
  background-color: var(--color-bg);
}

.chat-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 12px 20px;
  border-bottom: 1px solid var(--color-border);
  background-color: var(--color-surface);
}

.chat-title {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 16px;
  font-weight: 500;
}

.chat-model {
  font-size: 12px;
  color: var(--color-text-secondary);
  padding: 4px 8px;
  background-color: var(--color-border);
  border-radius: 4px;
}

.chat-empty {
  flex: 1;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 16px;
}

.empty-text {
  font-size: 16px;
  color: var(--color-text-secondary);
}

.messages-container {
  flex: 1;
  overflow-y: auto;
  padding: 20px;
  display: flex;
  flex-direction: column;
  gap: 20px;
}

.message {
  display: flex;
  gap: 12px;
  max-width: 80%;
}

.message.user {
  align-self: flex-end;
  flex-direction: row-reverse;
}

.message.user .message-content {
  background-color: var(--color-message-user);
  border-radius: 12px 4px 12px 12px;
}

.message.assistant .message-content {
  background-color: var(--color-message-assistant);
  border-radius: 4px 12px 12px 12px;
}

.message.tool .message-content {
  background-color: #2f3440;
  border-radius: 6px;
}

.message-avatar {
  width: 36px;
  height: 36px;
  display: flex;
  align-items: center;
  justify-content: center;
  background-color: var(--color-border);
  border-radius: 4px;
  flex-shrink: 0;
}

.message.user .message-avatar {
  background-color: var(--color-primary);
  color: white;
}

.message-content {
  padding: 12px 16px;
  max-width: 100%;
  overflow-wrap: break-word;
}

.message-text {
  font-size: 14px;
  line-height: 1.6;
}

.message-text :deep(pre) {
  background-color: #1a1a1a;
  padding: 12px;
  border-radius: 6px;
  overflow-x: auto;
  margin: 8px 0;
}

.message-text :deep(code) {
  font-family: 'Consolas', 'Monaco', monospace;
  font-size: 13px;
}

.typing-indicator {
  display: flex;
  gap: 4px;
  padding: 4px 0;
}

.typing-indicator span {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background-color: var(--color-text-secondary);
  animation: typing 1.4s infinite;
}

.typing-indicator span:nth-child(2) {
  animation-delay: 0.2s;
}

.typing-indicator span:nth-child(3) {
  animation-delay: 0.4s;
}

.reasoning-stream {
  margin-bottom: 10px;
  padding: 8px;
  border: 1px dashed var(--color-border);
  border-radius: 6px;
  color: var(--color-text-secondary);
  font-size: 12px;
  white-space: pre-wrap;
}

.tool-stream-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
  margin-bottom: 10px;
}

.tool-stream-card {
  border: 1px solid var(--color-border);
  border-radius: 6px;
  padding: 8px;
  background: rgba(255, 255, 255, 0.03);
}

.tool-stream-card.done {
  border-color: #2f9e44;
}

.tool-stream-card.error {
  border-color: #f03e3e;
}

.tool-title {
  font-size: 12px;
  font-weight: 600;
  margin-bottom: 4px;
}

.tool-args,
.tool-result {
  font-size: 12px;
  white-space: pre-wrap;
  word-break: break-word;
  color: var(--color-text-secondary);
}

@keyframes typing {
  0%, 60%, 100% {
    transform: translateY(0);
    opacity: 0.7;
  }
  30% {
    transform: translateY(-8px);
    opacity: 1;
  }
}

.input-container {
  padding: 16px 20px;
  border-top: 1px solid var(--color-border);
  background-color: var(--color-surface);
}

.input-wrapper {
  display: flex;
  gap: 12px;
  align-items: flex-end;
}

.input-wrapper :deep(.el-textarea) {
  flex: 1;
}

.input-wrapper :deep(.el-textarea__inner) {
  background-color: var(--color-bg);
  border-color: var(--color-border);
  color: var(--color-text);
  resize: none;
}

.input-wrapper :deep(.el-textarea__inner):focus {
  border-color: var(--color-primary);
}
</style>
