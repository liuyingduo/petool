<template>
  <div class="petool-app">
    <div class="bg-blob blob-a"></div>
    <div class="bg-blob blob-b"></div>

    <main
      ref="workspaceRef"
      class="workspace glass-panel"
      data-tauri-drag-region
      @mousedown.left="handleWorkspaceMouseDown"
    >
      <div
        class="drag-region"
        data-tauri-drag-region
        @mousedown.left.prevent="handleManualDrag"
      ></div>
      <div
        class="ear ear-left"
        data-tauri-drag-region
        aria-hidden="true"
        @mousedown.left.prevent="handleManualDrag"
      ></div>
      <div
        class="ear ear-right"
        data-tauri-drag-region
        aria-hidden="true"
        @mousedown.left.prevent="handleManualDrag"
      ></div>
      <div class="pet-eyes-container" aria-hidden="true">
        <div class="pet-eye">
          <div class="eye-pupil"></div>
        </div>
        <div class="pet-eye">
          <div class="eye-pupil"></div>
        </div>
      </div>
      <div class="window-controls" role="group" aria-label="窗口控制">
        <button class="window-control-btn" type="button" title="最小化" aria-label="最小化" @click="handleMinimize">
          <span class="material-icons-round">remove</span>
        </button>
        <button
          class="window-control-btn"
          type="button"
          :title="isWindowMaximized ? '退出全屏' : '全屏'"
          :aria-label="isWindowMaximized ? '退出全屏' : '全屏'"
          @click="handleToggleMaximize"
        >
          <span class="material-icons-round">{{ isWindowMaximized ? 'filter_none' : 'check_box_outline_blank' }}</span>
        </button>
        <button class="window-control-btn close" type="button" title="关闭" aria-label="关闭" @click="handleClose">
          <span class="material-icons-round">close</span>
        </button>
      </div>
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
        <div class="chat-body" :class="{ creating: createDialogVisible }">
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
import { invoke } from '@tauri-apps/api/core'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { useChatStore, type Message } from './stores/chat'
import { useConfigStore } from './stores/config'
import { useFilesystemStore } from './stores/filesystem'
import SettingsDialog from './components/Settings/index.vue'
import { registerChatEventListeners, type ReasoningEntry, type ToolStreamItem } from './composables/useChatEventBridge'
import { usePetWindowBehavior } from './composables/usePetWindowBehavior'

const chatStore = useChatStore()
const configStore = useConfigStore()
const fsStore = useFilesystemStore()

const inputMessage = ref('')
const newConversationTitle = ref('')
const showSettings = ref(false)
const createDialogVisible = ref(false)
const workspaceRef = ref<HTMLElement | null>(null)
const activeAssistantMessageId = ref<string | null>(null)
const reasoningByMessage = ref<Record<string, ReasoningEntry>>({})
const toolStreamItems = ref<ToolStreamItem[]>([])
const isWindowMaximized = ref(false)
const unlistenFns: Array<() => void> = []
const appWindow = getCurrentWindow()
const { handleManualDrag, handleWorkspaceMouseDown, setupCursorPassthrough, teardownCursorPassthrough } =
  usePetWindowBehavior(workspaceRef)

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

  unlistenFns.push(
    ...(await registerChatEventListeners({
      chatStore,
      activeAssistantMessageId,
      reasoningByMessage,
      toolStreamItems,
      onStreamEnd: collapseActiveReasoning
    }))
  )

  try {
    await syncWindowMaximizedState()
    const unlistenResize = await appWindow.listen('tauri://resize', () => {
      void syncWindowMaximizedState()
    })
    unlistenFns.push(unlistenResize)
  } catch {
    // ignore window control setup failures in non-Tauri runtime
  }

  setupCursorPassthrough()
})

onBeforeUnmount(() => {
  for (const unlisten of unlistenFns) {
    unlisten()
  }
  teardownCursorPassthrough()
})

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

async function syncWindowMaximizedState() {
  try {
    isWindowMaximized.value = await appWindow.isMaximized()
  } catch {
    isWindowMaximized.value = false
  }
}

async function handleMinimize() {
  try {
    await appWindow.minimize()
  } catch {
    // ignore unsupported runtime
  }
}

async function handleToggleMaximize() {
  try {
    await appWindow.toggleMaximize()
    await syncWindowMaximizedState()
  } catch {
    // ignore unsupported runtime
  }
}

async function handleClose() {
  try {
    await appWindow.close()
  } catch {
    // ignore unsupported runtime
  }
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

<style scoped src="./styles/app-shell.css"></style>

