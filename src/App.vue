<template>
  <div class="petool-app">
    <div class="bg-blob blob-a"></div>
    <div class="bg-blob blob-b"></div>

    <main class="workspace glass-panel">
      <div class="drag-region" data-tauri-drag-region></div>
      <div class="ear ear-left" aria-hidden="true"></div>
      <div class="ear ear-right" aria-hidden="true"></div>
      <aside class="sidebar">
        <button class="new-btn" @click="openCreateDialog">
          <span class="material-symbols-outlined">add_circle</span>
          开启新冒险
        </button>

        <div class="sidebar-title">进行中</div>

        <div class="conversation-list no-scrollbar">
          <button
            v-for="conv in chatStore.conversations"
            :key="conv.id"
            class="conv-item"
            :class="{ active: conv.id === chatStore.currentConversationId }"
            @click="handleSelectConversation(conv.id)"
          >
            <span class="dot"></span>
            <span class="material-icons-round">palette</span>
            <span class="conv-title">{{ conv.title }}</span>
          </button>

          <div v-if="chatStore.conversations.length === 0" class="empty-tip">还没有任务，先创建一个吧。</div>
        </div>

        <div class="sidebar-footer">
          <div class="user">Alex</div>
          <button class="settings-btn" @click="showSettings = true">
            <span class="material-icons-round">settings</span>
          </button>
        </div>
      </aside>

      <section class="chat-wrap">
        <div class="chat-body">
          <div v-if="createDialogVisible" class="create-mask"></div>

          <div v-if="createDialogVisible" class="create-dialog">
            <div class="dialog-head">
              <div class="dialog-title">Petool 肚子空空，准备开工！</div>
              <button class="dialog-close" @click="closeCreateDialog">
                <span class="material-icons-round">close</span>
              </button>
            </div>

            <label>任务名称</label>
            <input
              v-model="newConversationTitle"
              class="text-input"
              type="text"
              placeholder="例如：Q4 营销计划"
              @keydown.enter.prevent="handleCreateConversation"
            />

            <label>投喂文件夹</label>
            <button class="folder-zone" @click="handleSelectFolder">
              <span class="material-icons-round">folder_open</span>
              <span>把文件夹投喂给 Petool</span>
              <small>{{ fsStore.currentDirectory || '我会把它“吞下”，然后帮你干活！' }}</small>
            </button>

            <div v-if="recentFolders.length > 0" class="recent-wrap">
              <div class="recent-title">最近常吃</div>
              <div class="recent-list">
                <button
                  v-for="folder in recentFolders"
                  :key="folder"
                  class="recent-item"
                  @click="setFolderShortcut(folder)"
                >
                  {{ folder }}
                </button>
              </div>
            </div>

            <button class="start-btn" @click="handleCreateConversation">开饭啦！</button>
          </div>

          <div v-else class="message-list no-scrollbar">
            <div
              v-for="message in chatStore.currentMessages"
              :key="message.id"
              v-show="isRenderableMessage(message)"
              class="message-row"
              :class="message.role === 'user' ? 'user' : 'assistant'"
            >
              <div v-if="message.role === 'assistant'" class="message-meta">
                <span class="name">Petool</span>
                <span class="time">{{ formatTime(message.created_at) }}</span>
              </div>

              <div
                v-if="message.role === 'assistant' && getReasoningEntry(message.id)?.text"
                class="reasoning"
              >
                <button class="reasoning-toggle" @click="toggleReasoning(message.id)">
                  <span>思考过程</span>
                  <span class="reasoning-state">{{ isStreamingMessage(message.id) ? '思考中...' : '已折叠' }}</span>
                  <span class="material-icons-round">{{ getReasoningEntry(message.id)?.collapsed ? 'expand_more' : 'expand_less' }}</span>
                </button>
                <div v-show="!getReasoningEntry(message.id)?.collapsed" class="reasoning-content">
                  {{ getReasoningEntry(message.id)?.text }}
                </div>
              </div>

              <div v-if="shouldShowMessageBubble(message)" class="bubble">
                <div v-if="message.content.trim()" v-html="renderMarkdown(message.content)"></div>

                <div
                  v-if="message.role === 'assistant' && isStreamingMessage(message.id) && toolStreamItems.length > 0"
                  class="tool-list"
                >
                  <div v-for="item in toolStreamItems" :key="item.id" class="tool-item" :class="item.status">
                    <div class="tool-title">{{ item.name }}</div>
                    <div v-if="item.arguments" class="tool-text">{{ item.arguments }}</div>
                    <div v-if="item.result" class="tool-text">{{ item.result }}</div>
                  </div>
                </div>

                <div v-if="message.role === 'assistant' && isStreamingMessage(message.id)" class="typing">
                  <span></span><span></span><span></span>
                </div>
              </div>

              <div v-if="message.role === 'user'" class="read-status">已读</div>
            </div>
          </div>
        </div>

        <div class="input-bar" :class="{ disabled: createDialogVisible || !chatStore.currentConversationId }">
          <button class="attach-btn" @click="handleSelectFolder" :disabled="createDialogVisible">
            <span class="material-icons-round">attach_file</span>
          </button>
          <input
            v-model="inputMessage"
            type="text"
            placeholder="想让我做什么？"
            :disabled="createDialogVisible || !chatStore.currentConversationId || chatStore.streaming"
            @keydown.enter.prevent="sendMessage"
          />
          <button
            class="send-btn"
            :disabled="createDialogVisible || !chatStore.currentConversationId || chatStore.streaming || !inputMessage.trim()"
            @click="sendMessage"
          >
            <span class="material-icons-round">arrow_upward</span>
          </button>
        </div>
      </section>
    </main>

    <SettingsDialog v-model="showSettings" />
  </div>
</template>

<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref } from 'vue'
import { marked } from 'marked'
import { ElMessage } from 'element-plus'
import { listen } from '@tauri-apps/api/event'
import { invoke } from '@tauri-apps/api/core'
import { useChatStore, type Message } from './stores/chat'
import { useConfigStore } from './stores/config'
import { useFilesystemStore } from './stores/filesystem'
import SettingsDialog from './components/Settings/index.vue'

interface ReasoningEntry {
  text: string
  collapsed: boolean
}

const chatStore = useChatStore()
const configStore = useConfigStore()
const fsStore = useFilesystemStore()

const inputMessage = ref('')
const newConversationTitle = ref('')
const showSettings = ref(false)
const createDialogVisible = ref(false)
const activeAssistantMessageId = ref<string | null>(null)
const reasoningByMessage = ref<Record<string, ReasoningEntry>>({})
const toolStreamItems = ref<Array<{ id: string; name: string; arguments: string; result: string; status: 'running' | 'done' | 'error' }>>([])
const unlistenFns: Array<() => void> = []

const recentFolders = computed(() => {
  const paths = [fsStore.currentDirectory, configStore.config.work_directory]
    .filter((path): path is string => Boolean(path && path.trim()))
  return Array.from(new Set(paths)).slice(0, 3)
})

onMounted(async () => {
  await Promise.all([chatStore.loadConversations(), configStore.loadConfig()])
  if (chatStore.conversations.length > 0) {
    const first = chatStore.conversations[0]
    chatStore.setCurrentConversation(first.id)
    await chatStore.loadMessages(first.id)
    createDialogVisible.value = false
  } else {
    createDialogVisible.value = true
  }

  await registerChatListeners()
})

onBeforeUnmount(() => {
  for (const unlisten of unlistenFns) {
    unlisten()
  }
})

async function registerChatListeners() {
  unlistenFns.push(
    await listen('chat-chunk', (event) => {
      const chunk = event.payload as string
      if (!chatStore.currentConversationId) return
      chatStore.updateLastMessage(chatStore.currentConversationId, chunk)
    })
  )

  unlistenFns.push(
    await listen('chat-end', () => {
      chatStore.streaming = false
      collapseActiveReasoning()
      activeAssistantMessageId.value = null
      toolStreamItems.value = []
    })
  )

  unlistenFns.push(
    await listen('chat-reasoning', (event) => {
      const chunk = event.payload as string
      if (!chunk || !activeAssistantMessageId.value) return

      const id = activeAssistantMessageId.value
      if (!reasoningByMessage.value[id]) {
        reasoningByMessage.value[id] = { text: '', collapsed: false }
      }
      reasoningByMessage.value[id].text += chunk
      reasoningByMessage.value[id].collapsed = false
    })
  )

  unlistenFns.push(
    await listen('chat-tool-call', (event) => {
      const payload = event.payload as { toolCallId?: string; name?: string; argumentsChunk?: string }
      const id = payload.toolCallId || `tool-${Date.now()}`
      let item = toolStreamItems.value.find((entry) => entry.id === id)

      if (!item) {
        item = { id, name: payload.name || 'tool', arguments: '', result: '', status: 'running' }
        toolStreamItems.value.push(item)
      }

      if (payload.name) item.name = payload.name
      if (payload.argumentsChunk) item.arguments += payload.argumentsChunk
    })
  )

  unlistenFns.push(
    await listen('chat-tool-result', (event) => {
      const payload = event.payload as { toolCallId?: string; name?: string; result?: string | null; error?: string | null }
      const id = payload.toolCallId || `tool-${Date.now()}`
      let item = toolStreamItems.value.find((entry) => entry.id === id)

      if (!item) {
        item = { id, name: payload.name || 'tool', arguments: '', result: '', status: 'running' }
        toolStreamItems.value.push(item)
      }

      if (payload.name) item.name = payload.name
      item.status = payload.error ? 'error' : 'done'
      item.result = payload.error || payload.result || ''
    })
  )
}

function renderMarkdown(content: string) {
  return marked.parse(content || '', { async: false }) as string
}

function formatTime(isoString: string) {
  const date = new Date(isoString)
  return date.toLocaleTimeString('zh-CN', { hour: '2-digit', minute: '2-digit' })
}

function openCreateDialog() {
  newConversationTitle.value = ''
  createDialogVisible.value = true
}

function closeCreateDialog() {
  if (!chatStore.currentConversationId) return
  createDialogVisible.value = false
}

async function setFolderShortcut(folder: string) {
  fsStore.currentDirectory = folder
  const rootFiles = await fsStore.scanDirectory(folder)
  fsStore.files = rootFiles
  fsStore.children[folder] = rootFiles
}

async function handleSelectFolder() {
  try {
    await fsStore.selectFolder()
  } catch {
    ElMessage.error('选择文件夹失败')
  }
}

async function handleSelectConversation(id: string) {
  chatStore.setCurrentConversation(id)
  await chatStore.loadMessages(id)
  chatStore.streaming = false
  activeAssistantMessageId.value = null
  toolStreamItems.value = []
  createDialogVisible.value = false
}

async function handleCreateConversation() {
  const title = newConversationTitle.value.trim() || `新冒险 ${chatStore.conversations.length + 1}`

  try {
    const model = configStore.config.model || 'glm-4.7'
    const conversation = await chatStore.createConversation(title, model)
    chatStore.setCurrentConversation(conversation.id)
    await chatStore.loadMessages(conversation.id)
    inputMessage.value = ''
    createDialogVisible.value = false
  } catch (error) {
    ElMessage.error(getErrorMessage(error, '创建任务失败'))
  }
}

async function sendMessage() {
  const content = inputMessage.value.trim()
  if (!content || !chatStore.currentConversationId || chatStore.streaming) return

  const conversationId = chatStore.currentConversationId
  inputMessage.value = ''
  toolStreamItems.value = []

  const userMsg: Message = {
    id: Date.now().toString(),
    conversation_id: conversationId,
    role: 'user',
    content,
    created_at: new Date().toISOString()
  }
  chatStore.addMessage(conversationId, userMsg)

  const assistantMsg: Message = {
    id: (Date.now() + 1).toString(),
    conversation_id: conversationId,
    role: 'assistant',
    content: '',
    created_at: new Date().toISOString()
  }
  chatStore.addMessage(conversationId, assistantMsg)

  activeAssistantMessageId.value = assistantMsg.id
  chatStore.streaming = true

  try {
    await invoke('stream_message', { conversationId, content })
  } catch (error) {
    chatStore.streaming = false
    removePendingAssistantMessage(conversationId, assistantMsg.id)
    activeAssistantMessageId.value = null
    toolStreamItems.value = []
    ElMessage.error(getErrorMessage(error, '发送失败'))
  }
}

function removePendingAssistantMessage(conversationId: string, assistantId: string) {
  const messages = chatStore.messages[conversationId]
  if (!messages || messages.length === 0) return

  const index = messages.findIndex((message) => message.id === assistantId)
  if (index < 0) return

  const target = messages[index]
  if (target.role === 'assistant' && !target.content.trim()) {
    messages.splice(index, 1)
  }
}

function collapseActiveReasoning() {
  if (!activeAssistantMessageId.value) return
  const entry = reasoningByMessage.value[activeAssistantMessageId.value]
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
  return Boolean(chatStore.streaming && activeAssistantMessageId.value === messageId)
}

function isRenderableMessage(message: Message) {
  return message.role === 'assistant' || message.role === 'user'
}

function shouldShowMessageBubble(message: Message) {
  if (message.role === 'user') return true
  return Boolean(message.content.trim() || isStreamingMessage(message.id))
}

function getErrorMessage(error: unknown, fallback: string) {
  if (typeof error === 'string' && error.trim().length > 0) return error
  if (error instanceof Error && error.message.trim().length > 0) return error.message
  return fallback
}
</script>

<style scoped>
.petool-app {
  position: relative;
  width: 100vw;
  height: 100vh;
  display: flex;
  align-items: flex-start;
  justify-content: flex-start;
  overflow: hidden;
  padding: 0;
}

.bg-blob {
  position: absolute;
  border-radius: 999px;
  filter: blur(100px);
  opacity: 0.4;
}

.blob-a {
  width: 50vw;
  height: 50vw;
  top: -10vw;
  left: -10vw;
  background: #d8d1c0;
}

.blob-b {
  width: 60vw;
  height: 60vw;
  right: -10vw;
  bottom: -10vw;
  background: #c5cba8;
}

.workspace {
  width: 100%;
  height: calc(100% - 26px);
  margin-top: 26px;
  border-radius: 40px;
  overflow: hidden;
  display: flex;
  position: relative;
  z-index: 1;
}

.drag-region {
  position: absolute;
  top: -26px;
  left: 0;
  right: 0;
  height: 30px;
  z-index: 20;
}

.ear {
  position: absolute;
  top: -26px;
  width: 96px;
  height: 56px;
  border-radius: 48px 48px 0 0;
  background: rgba(253, 251, 247, 0.95);
  border: 1px solid rgba(255, 255, 255, 0.8);
  border-bottom: none;
  z-index: 5;
}

.ear::after {
  content: '';
  position: absolute;
  top: 16px;
  left: 50%;
  transform: translateX(-50%);
  width: 50px;
  height: 30px;
  border-radius: 50px 50px 0 0;
  background: #f3eee6;
  opacity: 0.85;
}

.ear-left {
  left: calc(50% - 166px);
}

.ear-right {
  left: calc(50% + 70px);
}

.glass-panel {
  background: rgba(253, 251, 247, 0.95);
  backdrop-filter: blur(18px);
  border: 1px solid rgba(255, 255, 255, 0.8);
  box-shadow: 0 25px 50px -12px rgba(44, 62, 51, 0.08);
}

.sidebar {
  width: 240px;
  border-right: 1px solid rgba(214, 211, 209, 0.7);
  background: rgba(243, 238, 230, 0.55);
  display: flex;
  flex-direction: column;
  padding: 16px;
  gap: 12px;
}

.new-btn {
  height: 48px;
  border: none;
  border-radius: 16px;
  background: #4a7c59;
  color: #fdfbf7;
  font-weight: 700;
  font-size: 14px;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 6px;
  cursor: pointer;
}

.sidebar-title {
  font-size: 11px;
  color: #9ca3af;
  text-transform: uppercase;
  letter-spacing: 0.08em;
  margin-top: 2px;
}

.conversation-list {
  flex: 1;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.conv-item {
  width: 100%;
  border: none;
  background: transparent;
  border-radius: 14px;
  padding: 12px;
  display: flex;
  align-items: center;
  gap: 8px;
  color: #78716c;
  cursor: pointer;
  text-align: left;
}

.conv-item.active {
  background: #ffffff;
  color: #2c3e33;
}

.dot {
  width: 6px;
  height: 6px;
  border-radius: 999px;
  background: transparent;
}

.conv-item.active .dot {
  background: #4a7c59;
}

.conv-title {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 14px;
}

.empty-tip {
  color: #9ca3af;
  font-size: 12px;
  padding: 8px;
}

.sidebar-footer {
  border-top: 1px solid rgba(214, 211, 209, 0.8);
  padding-top: 10px;
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.user {
  font-size: 14px;
  font-weight: 700;
  color: #44403c;
}

.settings-btn {
  border: none;
  background: transparent;
  color: #a8a29e;
  cursor: pointer;
}

.chat-wrap {
  flex: 1;
  display: flex;
  flex-direction: column;
  min-width: 0;
  background: rgba(253, 251, 247, 0.3);
}

.chat-body {
  flex: 1;
  min-height: 0;
  padding: 16px 24px;
  position: relative;
}

.create-mask {
  position: absolute;
  inset: 16px 24px;
  filter: blur(2px);
  opacity: 0.28;
  background: linear-gradient(180deg, rgba(255, 255, 255, 0), rgba(44, 62, 51, 0.08));
  pointer-events: none;
}

.create-dialog {
  position: relative;
  max-width: 560px;
  margin: 0 auto;
  background: #fdfbf7;
  border: 1px solid rgba(255, 255, 255, 0.9);
  border-radius: 38px;
  box-shadow: 0 30px 50px -25px rgba(0, 0, 0, 0.2);
  padding: 24px;
}

.dialog-head {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 16px;
}

.dialog-title {
  font-size: 20px;
  font-weight: 800;
  color: #2c3e33;
}

.dialog-close {
  border: none;
  background: #f5f5f4;
  color: #9ca3af;
  width: 30px;
  height: 30px;
  border-radius: 999px;
  cursor: pointer;
}

.text-input {
  width: 100%;
  border: 1px solid #e7e5e4;
  border-radius: 16px;
  background: #ffffff;
  font-size: 14px;
  color: #44403c;
  padding: 12px 16px;
  outline: none;
  margin-bottom: 12px;
}

.folder-zone {
  width: 100%;
  border: 2px dashed #6b9c7a;
  border-radius: 24px;
  min-height: 130px;
  background: #fcfbf9;
  padding: 12px;
  display: flex;
  flex-direction: column;
  justify-content: center;
  align-items: center;
  gap: 6px;
  cursor: pointer;
}

.folder-zone small {
  color: #9ca3af;
  max-width: 90%;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.recent-wrap {
  margin-top: 10px;
}

.recent-title {
  font-size: 10px;
  color: #9ca3af;
  margin-bottom: 6px;
}

.recent-list {
  display: flex;
  flex-wrap: wrap;
  gap: 8px;
}

.recent-item {
  border: 1px solid #f0ece7;
  border-radius: 12px;
  padding: 8px 10px;
  font-size: 12px;
  background: #fff;
  cursor: pointer;
  color: #57534e;
}

.start-btn {
  margin-top: 14px;
  width: 100%;
  height: 50px;
  border: none;
  border-radius: 16px;
  background: #4a7c59;
  color: #fff;
  font-size: 16px;
  font-weight: 700;
  cursor: pointer;
}

.message-list {
  height: 100%;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: 20px;
}

.message-row {
  max-width: 84%;
  display: flex;
  flex-direction: column;
}

.message-row.assistant {
  align-self: flex-start;
}

.message-row.user {
  align-self: flex-end;
}

.message-meta {
  margin-bottom: 4px;
  padding-left: 4px;
  display: flex;
  gap: 8px;
  align-items: center;
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
  margin-bottom: 8px;
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
  padding: 14px 16px;
  font-size: 14px;
  line-height: 1.65;
  color: #44403c;
  word-break: break-word;
}

.message-row.assistant .bubble {
  background: #fff;
  border: 1px solid #f5f5f4;
  border-top-left-radius: 8px;
}

.message-row.user .bubble {
  background: rgba(74, 124, 89, 0.12);
  border: 1px solid rgba(74, 124, 89, 0.12);
  border-top-right-radius: 8px;
}

.read-status {
  margin-top: 4px;
  align-self: flex-end;
  font-size: 10px;
  color: #9ca3af;
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

.tool-list {
  margin-top: 8px;
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
  font-size: 12px;
  color: #78716c;
  white-space: pre-wrap;
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

.input-bar {
  margin: 0 24px 20px;
  border-radius: 999px;
  background: #fff;
  border: 1px solid #f0ece7;
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 8px 8px 8px 14px;
}

.input-bar input {
  flex: 1;
  border: none;
  outline: none;
  font-size: 15px;
  background: transparent;
  color: #44403c;
}

.input-bar input::placeholder {
  color: #a8a29e;
}

.attach-btn,
.send-btn {
  border: none;
  width: 40px;
  height: 40px;
  border-radius: 999px;
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
}

.attach-btn {
  background: transparent;
  color: #a8a29e;
}

.send-btn {
  background: #4a7c59;
  color: #fff;
}

.send-btn:disabled,
.attach-btn:disabled {
  opacity: 0.45;
  cursor: not-allowed;
}

.input-bar.disabled {
  opacity: 0.5;
}

.no-scrollbar::-webkit-scrollbar {
  display: none;
}

.no-scrollbar {
  -ms-overflow-style: none;
  scrollbar-width: none;
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

@keyframes blink {
  0%,
  90%,
  100% {
    transform: scaleY(1);
  }

  95% {
    transform: scaleY(0.1);
  }
}

@keyframes scaleIn {
  from {
    opacity: 0;
    transform: scale(0.95);
  }

  to {
    opacity: 1;
    transform: scale(1);
  }
}

@media (max-width: 900px) {
  .workspace {
    flex-direction: column;
    height: calc(100% - 26px);
  }

  .sidebar {
    width: 100%;
    max-height: 230px;
    border-right: none;
    border-bottom: 1px solid rgba(214, 211, 209, 0.7);
  }

  .chat-wrap {
    border-top: none;
  }

  .ear {
    display: none;
  }
}
</style>
