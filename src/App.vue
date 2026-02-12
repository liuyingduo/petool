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
      <div class="pet-eyes-container" :class="{ 'is-asking': Boolean(activeToolApproval) }" aria-hidden="true">
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
          :title="isWindowMaximized ? '还原' : '最大化'"
          :aria-label="isWindowMaximized ? '还原' : '最大化'"
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
          <div class="user">用户</div>
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
              <div class="dialog-title">Petool 请求你的指引</div>
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

            <label>工作区文件夹</label>
            <button class="folder-zone" @click="handleSelectFolder">
              <span class="material-icons-round">folder_open</span>
              <span>把文件夹交给 Petool</span>
              <small>{{ fsStore.currentDirectory || '我会在这个工作区里帮你完成任务。' }}</small>
            </button>

            <div v-if="recentFolders.length > 0" class="recent-wrap">
              <div class="recent-title">最近使用</div>
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

            <button class="start-btn" @click="handleCreateConversation">开始</button>
          </div>

          <div v-else class="message-list no-scrollbar" @click="handleMarkdownLinkClick">
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

        <div v-if="activeToolApproval" class="tool-approval-card">
          <div class="tool-approval-header">
            <div class="tool-approval-title">{{ approvalTitle }}</div>
            <div class="tool-approval-subtitle">{{ approvalSubtitle }}</div>
          </div>

          <div v-if="approvalFolderCard" class="approval-folder-wrap">
            <div class="approval-folder-card">
              <div class="approval-folder-icon">
                <span class="material-icons-round">folder</span>
              </div>
              <div class="approval-folder-content">
                <div class="approval-folder-name">{{ approvalFolderCard.name }}</div>
                <div class="approval-folder-path">{{ approvalFolderCard.location }}</div>
              </div>
            </div>
          </div>

          <div v-else-if="approvalDetailText" class="tool-approval-detail">
            {{ approvalDetailText }}
          </div>

          <pre
            v-if="activeToolApproval.arguments && !approvalFolderCard && !approvalDetailText"
            class="tool-approval-args"
          >{{ activeToolApproval.arguments }}</pre>

          <div class="tool-approval-actions">
            <button
              class="tool-approval-btn deny"
              :disabled="resolvingToolApproval"
              @click="resolveToolApproval('deny')"
            >
              先不要看
            </button>
            <button
              class="tool-approval-btn trust"
              :disabled="resolvingToolApproval"
              @click="resolveToolApproval('allow_always')"
            >
              以后都相信你
            </button>
            <button
              class="tool-approval-btn primary"
              :disabled="resolvingToolApproval"
              @click="resolveToolApproval('allow_once')"
            >
              准许执行 ✅
            </button>
          </div>
        </div>

        <div v-if="pendingUploads.length > 0" class="upload-strip">
          <div class="upload-strip-title">已添加文件（发送后会一并交给模型）</div>
          <div class="upload-list">
            <div v-for="item in pendingUploads" :key="item.id" class="upload-chip">
              <span class="material-icons-round">{{ item.inlineText ? 'description' : 'insert_drive_file' }}</span>
              <span class="upload-chip-name">{{ item.name }}</span>
              <span class="upload-chip-meta">{{ formatBytes(item.size) }}</span>
              <button class="upload-chip-remove" type="button" @click.stop="removeUpload(item.id)">
                <span class="material-icons-round">close</span>
              </button>
            </div>
          </div>
        </div>

        <div class="input-bar" :class="{ disabled: createDialogVisible || !chatStore.currentConversationId }">
          <button class="attach-btn" @click="handleSelectUploadFiles" :disabled="createDialogVisible || chatStore.streaming">
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
            :disabled="createDialogVisible || !chatStore.currentConversationId || chatStore.streaming || !canSendMessage"
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
import { open as openDialog } from '@tauri-apps/plugin-dialog'
import { open as openExternal } from '@tauri-apps/plugin-shell'
import { useChatStore, type Message } from './stores/chat'
import { useConfigStore } from './stores/config'
import { useFilesystemStore } from './stores/filesystem'
import SettingsDialog from './components/Settings/index.vue'
import {
  registerChatEventListeners,
  type ReasoningEntry,
  type ToolApprovalRequest,
  type ToolStreamItem
} from './composables/useChatEventBridge'
import { usePetWindowBehavior } from './composables/usePetWindowBehavior'

interface UploadAttachment {
  id: string
  path: string
  name: string
  extension: string
  size: number
  inlineText: string | null
  inlineTruncated: boolean
  note: string
}

interface PathInfo {
  name: string
  path: string
  is_dir: boolean
  size?: number
  extension?: string
}

const MAX_INLINE_FILE_SIZE = 1_500_000
const MAX_INLINE_TEXT_CHARS = 80_000
const MAX_TOTAL_INLINE_CHARS = 140_000

const TEXT_FILE_EXTENSIONS = new Set([
  'txt', 'md', 'markdown', 'json', 'jsonl', 'yaml', 'yml', 'toml', 'ini', 'csv', 'tsv',
  'xml', 'html', 'htm', 'css', 'js', 'mjs', 'cjs', 'ts', 'tsx', 'jsx', 'vue',
  'py', 'java', 'go', 'rs', 'cpp', 'c', 'h', 'hpp', 'sh', 'ps1', 'bat', 'sql', 'log'
])

const BINARY_FILE_EXTENSIONS = new Set([
  'pdf', 'doc', 'docx', 'ppt', 'pptx', 'xls', 'xlsx', 'xlsm', 'zip', 'rar', '7z',
  'png', 'jpg', 'jpeg', 'gif', 'bmp', 'webp', 'mp3', 'wav', 'mp4', 'mov'
])

const chatStore = useChatStore()
const configStore = useConfigStore()
const fsStore = useFilesystemStore()

const inputMessage = ref('')
const newConversationTitle = ref('')
const showSettings = ref(false)
const createDialogVisible = ref(false)
const pendingUploads = ref<UploadAttachment[]>([])
const workspaceRef = ref<HTMLElement | null>(null)
const activeAssistantMessageId = ref<string | null>(null)
const reasoningByMessage = ref<Record<string, ReasoningEntry>>({})
const toolStreamItems = ref<ToolStreamItem[]>([])
const pendingToolApproval = ref<ToolApprovalRequest | null>(null)
const resolvingToolApproval = ref(false)
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

const canSendMessage = computed(() => {
  return inputMessage.value.trim().length > 0 || pendingUploads.value.length > 0
})

const activeToolApproval = computed(() => {
  const request = pendingToolApproval.value
  if (!request || !chatStore.currentConversationId) return null
  return request.conversationId === chatStore.currentConversationId ? request : null
})

const parsedApprovalArgs = computed<Record<string, unknown>>(() => {
  const raw = activeToolApproval.value?.arguments
  if (!raw) return {}
  try {
    const parsed = JSON.parse(raw)
    if (parsed && typeof parsed === 'object') {
      return parsed as Record<string, unknown>
    }
  } catch {
    // ignore parse errors and fallback to empty args
  }
  return {}
})

const approvalFolderCard = computed<{ name: string; location: string } | null>(() => {
  const request = activeToolApproval.value
  if (!request || normalizeToolName(request.toolName) !== 'workspace_list_directory') return null

  const args = parsedApprovalArgs.value
  const requestedPath = typeof args.path === 'string' && args.path.trim() ? args.path.trim() : '.'
  const workspaceRoot = fsStore.currentDirectory || configStore.config.work_directory || ''
  const resolvedPath = workspaceRoot
    ? resolveRequestedPath(workspaceRoot, requestedPath)
    : requestedPath

  const name = getPathName(resolvedPath) || '当前目录'
  return {
    name,
    location: truncateMiddle(resolvedPath, 68)
  }
})

const approvalTitle = computed(() => {
  const request = activeToolApproval.value
  if (!request) return ''

  const toolName = normalizeToolName(request.toolName)
  if (toolName === 'workspace_list_directory') return 'Petool 想先看看这里...'
  if (toolName === 'skills_install_from_repo') return 'Petool 想帮你安装一个技能'
  if (toolName.startsWith('mcp__')) return 'Petool 想调用外部工具'
  return 'Petool 请求你的指引'
})

const approvalSubtitle = computed(() => {
  const request = activeToolApproval.value
  if (!request) return ''

  const toolName = normalizeToolName(request.toolName)

  if (toolName === 'workspace_list_directory') {
    return '为了完成任务，我需要先查看这个文件夹。'
  }
  if (toolName === 'workspace_read_file') {
    return '为了继续处理，我需要先读取这个文件。'
  }
  if (toolName === 'workspace_write_file') {
    return '为了应用你的修改请求，我需要写入这个文件。'
  }
  if (toolName === 'workspace_run_command') {
    return '为了完成你的请求，我需要运行一条本地命令。'
  }
  if (toolName === 'skills_install_from_repo') {
    return '为了解决当前问题，我希望从仓库安装一个技能。'
  }
  if (toolName.startsWith('mcp__')) {
    return `我需要调用外部工具：${renderToolLabel(request.toolName)}`
  }
  return `我即将调用工具：${renderToolLabel(request.toolName)}`
})

const approvalDetailText = computed(() => {
  const request = activeToolApproval.value
  if (!request || approvalFolderCard.value) return ''

  const toolName = normalizeToolName(request.toolName)
  const args = parsedApprovalArgs.value

  if ((toolName === 'workspace_read_file' || toolName === 'workspace_write_file') && typeof args.path === 'string') {
    return `路径：${truncateMiddle(args.path, 72)}`
  }
  if (toolName === 'workspace_run_command' && typeof args.command === 'string') {
    return `命令：${truncateMiddle(args.command, 72)}`
  }
  if (toolName === 'skills_install_from_repo') {
    const repoUrlRaw =
      (typeof args.repo_url === 'string' && args.repo_url.trim()) ||
      (typeof args.repoUrl === 'string' && args.repoUrl.trim())
    if (repoUrlRaw) {
      return `仓库：${truncateMiddle(repoUrlRaw, 72)}`
    }
  }
  if (toolName.startsWith('mcp__')) {
    return `工具：${renderToolLabel(request.toolName)}`
  }

  return ''
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
      onToolApprovalRequest: (request) => {
        pendingToolApproval.value = request
      },
      onStreamEnd: () => {
        collapseActiveReasoning()
        pendingToolApproval.value = null
        resolvingToolApproval.value = false
      }
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

function isExternalHttpUrl(value: string) {
  return /^https?:\/\//i.test(value)
}

async function handleMarkdownLinkClick(event: MouseEvent) {
  const target = event.target
  if (!(target instanceof HTMLElement)) return

  const anchor = target.closest('a') as HTMLAnchorElement | null
  if (!anchor) return

  const href = (anchor.getAttribute('href') || '').trim()
  if (!href || !isExternalHttpUrl(href)) return

  event.preventDefault()
  event.stopPropagation()

  try {
    await openExternal(href)
  } catch (error) {
    ElMessage.error(getErrorMessage(error, '打开外部链接失败'))
  }
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

async function handleSelectUploadFiles() {
  try {
    const selected = await openDialog({
      title: '选择要分析的文件',
      multiple: true,
      directory: false
    })

    if (!selected) return

    const paths = Array.isArray(selected) ? selected : [selected]
    const uniquePaths = paths.filter((path, index) => path && paths.indexOf(path) === index)

    for (const path of uniquePaths) {
      if (pendingUploads.value.some((item) => item.path === path)) continue
      const attachment = await buildUploadAttachment(path)
      pendingUploads.value.push(attachment)
    }
  } catch (error) {
    ElMessage.error(getErrorMessage(error, '选择文件失败'))
  }
}

function removeUpload(uploadId: string) {
  pendingUploads.value = pendingUploads.value.filter((item) => item.id !== uploadId)
}

async function handleSelectConversation(id: string) {
  chatStore.setCurrentConversation(id)
  await chatStore.loadMessages(id)
  chatStore.streaming = false
  activeAssistantMessageId.value = null
  toolStreamItems.value = []
  pendingToolApproval.value = null
  resolvingToolApproval.value = false
  createDialogVisible.value = false
}

async function resolveToolApproval(decision: 'allow_once' | 'allow_always' | 'deny') {
  const request = activeToolApproval.value
  if (!request || resolvingToolApproval.value) return

  resolvingToolApproval.value = true
  try {
    await invoke('resolve_tool_approval', {
      requestId: request.requestId,
      decision
    })
    pendingToolApproval.value = null
  } catch (error) {
    ElMessage.error(getErrorMessage(error, '处理工具权限失败'))
  } finally {
    resolvingToolApproval.value = false
  }
}

async function handleCreateConversation() {
  const title = newConversationTitle.value.trim() || `新冒险 ${chatStore.conversations.length + 1}`

  try {
    const model = configStore.config.model || 'glm-5'
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
  const uploads = [...pendingUploads.value]
  if ((!content && uploads.length === 0) || !chatStore.currentConversationId || chatStore.streaming) return

  const conversationId = chatStore.currentConversationId
  const messageContentForModel = buildMessageForModel(content, uploads)
  const messageContentForView = buildMessageForDisplay(content, uploads)
  const workspaceDirectory = resolveWorkspaceDirectoryForSend(uploads)

  inputMessage.value = ''
  pendingUploads.value = []
  toolStreamItems.value = []
  pendingToolApproval.value = null

  const userMsg: Message = {
    id: Date.now().toString(),
    conversation_id: conversationId,
    role: 'user',
    content: messageContentForView,
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
    await invoke('stream_message', {
      conversationId,
      content: messageContentForModel,
      workspaceDirectory
    })
  } catch (error) {
    chatStore.streaming = false
    removePendingAssistantMessage(conversationId, assistantMsg.id)
    activeAssistantMessageId.value = null
    toolStreamItems.value = []
    pendingToolApproval.value = null
    resolvingToolApproval.value = false
    inputMessage.value = content
    pendingUploads.value = uploads
    ElMessage.error(getErrorMessage(error, '发送消息失败'))
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

async function buildUploadAttachment(path: string): Promise<UploadAttachment> {
  const pathInfo = await invoke<PathInfo>('get_path_info', { path })
  if (pathInfo.is_dir) {
    throw new Error(`不是文件: ${path}`)
  }

  const extension = (pathInfo.extension || getPathExtension(path)).toLowerCase()
  const size = typeof pathInfo.size === 'number' && Number.isFinite(pathInfo.size) ? pathInfo.size : 0
  const isLikelyBinary = BINARY_FILE_EXTENSIONS.has(extension)
  const isLikelyText = TEXT_FILE_EXTENSIONS.has(extension)
  const attachment: UploadAttachment = {
    id: `${Date.now()}-${Math.random().toString(16).slice(2, 8)}`,
    path,
    name: getPathName(path) || path,
    extension,
    size,
    inlineText: null,
    inlineTruncated: false,
    note: ''
  }

  if (isLikelyBinary) {
    attachment.note = '该文件为二进制格式，将以文件路径方式交给模型分析。'
    return attachment
  }

  if (size > MAX_INLINE_FILE_SIZE) {
    attachment.note = '文件较大，已仅上传文件路径与元信息。'
    return attachment
  }

  if (!isLikelyText && extension) {
    attachment.note = '该类型默认按路径交给模型分析。'
    return attachment
  }

  try {
    const rawText = await invoke<string>('read_file', { path })
    if (!rawText.trim()) {
      attachment.note = '文件内容为空，将只提供文件路径。'
      return attachment
    }

    if (rawText.length > MAX_INLINE_TEXT_CHARS) {
      attachment.inlineText = rawText.slice(0, MAX_INLINE_TEXT_CHARS)
      attachment.inlineTruncated = true
      attachment.note = '文件内容过长，已截断后上传。'
    } else {
      attachment.inlineText = rawText
      attachment.note = '文件内容已上传给模型。'
    }
  } catch {
    attachment.note = '读取文本失败，将以文件路径方式交给模型分析。'
  }

  return attachment
}

function buildMessageForDisplay(content: string, uploads: UploadAttachment[]) {
  const trimmed = content.trim()
  if (uploads.length === 0) return trimmed

  const fileLines = uploads.map((item) => `- ${item.name}`)
  const filesBlock = `\n\n[已上传文件]\n${fileLines.join('\n')}`
  return `${trimmed || '请分析我上传的文件。'}${filesBlock}`
}

function buildMessageForModel(content: string, uploads: UploadAttachment[]) {
  if (uploads.length === 0) return content.trim()

  let remainingInlineBudget = MAX_TOTAL_INLINE_CHARS
  const lines: string[] = []
  lines.push('【用户上传文件】')

  for (let i = 0; i < uploads.length; i += 1) {
    const item = uploads[i]
    lines.push(`${i + 1}. 文件名: ${item.name}`)
    lines.push(`   路径: ${item.path}`)
    lines.push(`   大小: ${formatBytes(item.size)}`)
    lines.push(`   说明: ${item.note}`)

    if (item.inlineText && remainingInlineBudget > 0) {
      const textToInclude =
        item.inlineText.length > remainingInlineBudget
          ? item.inlineText.slice(0, remainingInlineBudget)
          : item.inlineText
      remainingInlineBudget -= textToInclude.length
      lines.push('   内容片段:')
      lines.push('```text')
      lines.push(textToInclude)
      lines.push('```')
      if (item.inlineTruncated || textToInclude.length < item.inlineText.length) {
        lines.push('   注: 内容已截断。')
      }
    } else {
      lines.push('   内容片段: 未内联，请通过文件路径读取。')
    }
  }

  lines.push('请优先基于上述附件内容完成分析；若内容未内联，请通过可用工具读取该路径文件。')

  const userText = content.trim() || '请分析这些文件，并给出清晰结论。'
  return `${userText}\n\n${lines.join('\n')}`
}

function resolveWorkspaceDirectoryForSend(uploads: UploadAttachment[]) {
  const configuredWorkspace = fsStore.currentDirectory || configStore.config.work_directory || null
  if (uploads.length === 0) return configuredWorkspace

  const firstParent = getParentPath(uploads[0].path)
  if (!firstParent) return configuredWorkspace
  if (!configuredWorkspace) return firstParent

  return isPathInside(configuredWorkspace, uploads[0].path) ? configuredWorkspace : firstParent
}

function isPathInside(basePath: string, targetPath: string) {
  const base = normalizePathForCompare(basePath)
  const target = normalizePathForCompare(targetPath)
  return target === base || target.startsWith(`${base}/`)
}

function normalizePathForCompare(value: string) {
  return value.replace(/\\/g, '/').replace(/\/+$/, '').toLowerCase()
}

function getParentPath(input: string) {
  const value = input.trim().replace(/[\\/]+$/, '')
  if (!value) return ''
  const parts = value.split(/[\\/]+/)
  if (parts.length <= 1) return ''
  const sep = value.includes('\\') ? '\\' : '/'
  const parent = parts.slice(0, parts.length - 1).join(sep)
  if (/^[a-zA-Z]:$/.test(parent)) return `${parent}\\`
  return parent
}

function getPathExtension(input: string) {
  const name = getPathName(input).toLowerCase()
  const dot = name.lastIndexOf('.')
  if (dot < 0 || dot === name.length - 1) return ''
  return name.slice(dot + 1)
}

function formatBytes(bytes: number) {
  if (!Number.isFinite(bytes) || bytes <= 0) return '0 B'
  const units = ['B', 'KB', 'MB', 'GB']
  let value = bytes
  let unitIndex = 0
  while (value >= 1024 && unitIndex < units.length - 1) {
    value /= 1024
    unitIndex += 1
  }
  return `${value.toFixed(value >= 10 || unitIndex === 0 ? 0 : 1)} ${units[unitIndex]}`
}

function normalizeToolName(name: string) {
  const raw = name.trim()
  if (!raw) return raw

  if (raw.startsWith('workspace_') || raw.startsWith('skills_')) return raw

  const mcpPrefixMatch = raw.match(/^mcp__[^_]+__(.+)$/)
  if (mcpPrefixMatch && mcpPrefixMatch[1]) {
    return `mcp__${mcpPrefixMatch[1]}`
  }

  return raw
}

function renderToolLabel(name: string) {
  const raw = name.trim()
  if (!raw) return '未知工具'

  if (raw.startsWith('mcp__')) {
    const parts = raw.split('__')
    if (parts.length >= 3) {
      const server = parts[1] || 'mcp'
      const tool = parts.slice(2).join('__') || 'tool'
      return `${server}.${tool}`
    }
  }

  return raw
}

function resolveRequestedPath(base: string, target: string) {
  const cleanedBase = base.trim()
  const cleanedTarget = target.trim()
  if (!cleanedBase) return cleanedTarget
  if (!cleanedTarget || cleanedTarget === '.') return cleanedBase

  const isAbsoluteTarget =
    /^[a-zA-Z]:[\\/]/.test(cleanedTarget) || cleanedTarget.startsWith('\\\\') || cleanedTarget.startsWith('/')
  if (isAbsoluteTarget) return cleanedTarget

  const sep = cleanedBase.includes('\\') ? '\\' : '/'
  const normalizedBase = cleanedBase.replace(/[\\/]+$/, '')
  const normalizedTarget = cleanedTarget.replace(/^[\\/]+/, '')
  return `${normalizedBase}${sep}${normalizedTarget}`
}

function getPathName(input: string) {
  const value = input.trim().replace(/[\\/]+$/, '')
  if (!value) return ''
  const parts = value.split(/[\\/]+/)
  return parts[parts.length - 1] || ''
}

function truncateMiddle(input: string, max = 64) {
  if (input.length <= max) return input
  const head = Math.ceil((max - 1) / 2)
  const tail = Math.floor((max - 1) / 2)
  return `${input.slice(0, head)}...${input.slice(input.length - tail)}`
}
</script>

<style scoped src="./styles/app-shell.css"></style>



