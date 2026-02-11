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
          <el-icon v-else><Robot /></el-icon>
        </div>
        <div class="message-content">
          <div class="message-text" v-html="renderMarkdown(message.content)"></div>
        </div>
      </div>

      <!-- Loading indicator -->
      <div v-if="chatStore.streaming" class="message assistant">
        <div class="message-avatar">
          <el-icon><Robot /></el-icon>
        </div>
        <div class="message-content">
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
import { ChatDotRound, User, Robot, Promotion } from '@element-plus/icons-vue'
import { marked } from 'marked'
import { ElMessage } from 'element-plus'
import { listen } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'

const chatStore = useChatStore()
const inputMessage = ref('')

// Listen for streaming events
listen('chat-chunk', (event) => {
  const chunk = event.payload as string
  chatStore.updateLastMessage(chatStore.currentConversationId!, chunk)
})

listen('chat-end', () => {
  chatStore.streaming = false
})

function renderMarkdown(content: string) {
  return marked(content)
}

async function sendMessage() {
  const content = inputMessage.value.trim()
  if (!content || chatStore.streaming) return

  inputMessage.value = ''

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
    const window = getCurrentWindow()
    await window.invoke('stream_message', {
      conversationId: chatStore.currentConversationId,
      content,
    })
  } catch (error) {
    chatStore.streaming = false
    ElMessage.error(typeof error === 'string' ? error : 'Failed to send message')
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
