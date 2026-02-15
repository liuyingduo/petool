<template>
  <div class="petool-app" :class="{ 'custom-chrome': useCustomWindowChrome }">
    <main
      ref="workspaceRef"
      class="workspace"
      @mousedown.left="handleWorkspaceMouseDown"
    >
      <div
        v-if="useCustomWindowChrome"
        class="drag-region"
        data-tauri-drag-region
        @mousedown.left.prevent="handleManualDrag"
      ></div>
      <div
        v-if="useCustomWindowChrome"
        class="pet-eyes-container"
        :class="{ 'is-asking': Boolean(activeToolApproval) }"
        aria-hidden="true"
      >
        <div class="pet-eye">
          <div class="eye-pupil"></div>
        </div>
        <div class="pet-eye">
          <div class="eye-pupil"></div>
        </div>
      </div>
      <div v-if="useCustomWindowChrome" class="window-controls" role="group" aria-label="窗口控制">
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
      <div class="workspace-shell glass-panel">
        <aside class="sidebar">
          <button class="new-btn" @click="openCreateDialog">
            <span class="material-symbols-outlined">add_circle</span>
            开启新冒险
          </button>

        <div class="sidebar-title">进行中</div>

        <div class="conversation-list no-scrollbar">
          <div
            v-for="(conv, index) in conversationsForDisplay"
            :key="conv.id"
            class="conv-item-row"
            :class="{ active: conv.id === chatStore.currentConversationId }"
          >
            <button
              class="conv-item"
              :class="{ active: conv.id === chatStore.currentConversationId }"
              @click="handleSelectConversation(conv.id)"
            >
              <span class="dot"></span>
              <span class="material-icons-round">{{ getConversationIcon(index) }}</span>
              <span class="conv-title">{{ conv.title }}</span>
            </button>
            <div class="conv-menu-anchor">
              <el-dropdown
                trigger="click"
                placement="bottom-end"
                popper-class="conv-actions-menu"
                @command="handleConversationMenuCommandById(conv.id, $event)"
              >
                <button
                  class="conv-menu-trigger"
                  type="button"
                  title="会话操作"
                  aria-label="会话操作"
                  :disabled="chatStore.isConversationStreaming(conv.id)"
                  @click.stop
                >
                  <span class="conv-menu-dot" aria-hidden="true"></span>
                  <span class="conv-menu-dot" aria-hidden="true"></span>
                  <span class="conv-menu-dot" aria-hidden="true"></span>
                </button>
                <template #dropdown>
                  <el-dropdown-menu>
                    <el-dropdown-item command="pin">
                      {{ isConversationPinned(conv.id) ? '取消置顶' : '置顶' }}
                    </el-dropdown-item>
                    <el-dropdown-item command="rename">重命名</el-dropdown-item>
                    <el-dropdown-item command="delete" class="danger" divided>删除</el-dropdown-item>
                  </el-dropdown-menu>
                </template>
              </el-dropdown>
            </div>
          </div>

          <div v-if="chatStore.conversations.length === 0" class="empty-tip">还没有任务，先创建一个吧。</div>
        </div>

        <div class="sidebar-footer">
          <div class="sidebar-user">
            <div class="sidebar-avatar-wrap">
              <img class="sidebar-avatar" :src="userAvatarUrl" alt="User Avatar" />
            </div>
            <div class="sidebar-user-meta">
              <span class="sidebar-user-name">Alex</span>
              <span class="sidebar-user-plan">Pro Plan</span>
            </div>
          </div>
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
              <small>{{ createConversationWorkspaceDirectory || configStore.config.work_directory || '我会在这个工作区里帮你完成任务。' }}</small>
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

          <div
            v-else
            ref="messageListRef"
            class="message-list no-scrollbar"
            @click="handleMarkdownLinkClick"
            @scroll.passive="handleMessageListScroll"
          >
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
              <div v-else class="message-meta user-meta">
                <span class="time">{{ formatTime(message.created_at) }}</span>
                <div class="message-avatar">
                  <img class="avatar-img" :src="userAvatarUrl" alt="User Avatar" />
                </div>
              </div>

              <div v-if="shouldShowMessageBubble(message)" class="bubble">
                <div
                  v-if="shouldShowToolProgress(message)"
                  class="tool-progress"
                >
                  <div v-if="isToolDisplayFull" class="tool-list">
                    <div
                      v-for="item in getToolItemsForMessage(message.id)"
                      :key="`${message.id}-${item.id}`"
                      class="tool-item"
                      :class="item.status"
                    >
                      <div class="tool-title">{{ renderToolItemName(item) }}</div>
                      <div v-if="item.arguments" class="tool-text">
                        <span class="tool-text-label">参数</span>
                        <pre class="tool-code">{{ renderToolArguments(item) }}</pre>
                      </div>
                      <div v-if="item.result" class="tool-text">
                        <span class="tool-text-label">结果</span>
                        <pre class="tool-code">{{ renderToolResult(item) }}</pre>
                      </div>
                    </div>
                  </div>

                  <div v-else class="tool-compact">
                    <button class="tool-compact-head" type="button" @click="toggleCompactToolList(message.id)">
                      <span class="tool-compact-title">{{ getCompactToolSummaryTitle(message.id) }}</span>
                      <span class="tool-compact-count">{{ getCompactToolSummaryCount(message.id) }}</span>
                      <span class="material-icons-round">
                        {{ isToolListCollapsed(message.id) ? 'expand_more' : 'expand_less' }}
                      </span>
                    </button>
                    <div v-show="!isToolListCollapsed(message.id)" class="tool-compact-list">
                      <div
                        v-for="item in getToolItemsForMessage(message.id)"
                        :key="`compact-${message.id}-${item.id}`"
                        class="tool-compact-item"
                        :class="item.status"
                      >
                        <span class="tool-compact-dot" aria-hidden="true"></span>
                        <span class="tool-compact-name">{{ renderToolItemName(item) }}</span>
                        <span class="tool-compact-status">{{ getToolStatusLabel(item.status) }}</span>
                      </div>
                    </div>
                  </div>
                </div>

                <div
                  v-if="message.role === 'assistant' && getReasoningEntry(message.id)?.text"
                  class="reasoning"
                >
                  <button class="reasoning-toggle" @click="toggleReasoning(message.id)">
                    <span>思考过程</span>
                    <span class="reasoning-state">{{ isStreamingMessage(message.id) ? '正在思考中...' : '已折叠' }}</span>
                    <span class="material-icons-round">{{ getReasoningEntry(message.id)?.collapsed ? 'expand_more' : 'expand_less' }}</span>
                  </button>
                  <div v-show="!getReasoningEntry(message.id)?.collapsed" class="reasoning-content">
                    {{ getReasoningEntry(message.id)?.text }}
                  </div>
                </div>

                <div
                  v-if="shouldRenderAssistantContent(message)"
                  v-html="renderMarkdown(getDisplayedMessageContent(message))"
                ></div>

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
          <div class="model-selector">
            <button
              class="model-trigger"
              type="button"
              :disabled="createDialogVisible || isCurrentConversationStreaming"
              aria-label="选择模型"
            >
              <span class="model-dot"></span>
              <span class="model-text">{{ activeModelLabel }}</span>
              <span class="material-icons-round">expand_more</span>
            </button>
            <div class="model-dropdown">
              <div class="model-dropdown-title">选择模型</div>
              <button
                v-for="model in modelOptions"
                :key="model"
                class="model-option"
                type="button"
                :class="{ active: model === activeModelId }"
                @click="handleSelectModel(model)"
              >
                <span>{{ formatModelLabel(model) }}</span>
                <span v-if="model === activeModelId" class="material-icons-round">check</span>
              </button>
            </div>
          </div>
          <button class="attach-btn" @click="handleSelectUploadFiles" :disabled="createDialogVisible || isCurrentConversationStreaming">
            <span class="material-icons-round">attach_file</span>
          </button>
          <button
            class="image-btn"
            :disabled="
              createDialogVisible ||
              !chatStore.currentConversationId ||
              isCurrentConversationStreaming ||
              generatingImage ||
              !canGenerateImagePrompt
            "
            @click="generateImage"
          >
            <span class="material-icons-round">{{ generatingImage ? 'hourglass_top' : 'image' }}</span>
          </button>
          <input
            v-model="inputMessage"
            type="text"
            placeholder="想让我做什么？"
            :disabled="createDialogVisible || !chatStore.currentConversationId || isCurrentConversationStreaming"
            @keydown.enter.prevent="sendMessage"
          />
          <button
            class="send-btn"
            :disabled="
              createDialogVisible ||
              !chatStore.currentConversationId ||
              (isCurrentConversationStreaming ? pausingStream : !canSendMessage)
            "
            @click="isCurrentConversationStreaming ? pauseStream() : sendMessage()"
          >
            <span v-if="isCurrentConversationStreaming" class="send-stop-square" aria-hidden="true"></span>
            <span v-else class="material-icons-round">arrow_upward</span>
          </button>
        </div>
        </section>
      </div>
    </main>

    <SettingsDialog v-model="showSettings" />
  </div>
</template>

<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { marked } from 'marked'
import { ElMessage, ElMessageBox } from 'element-plus'
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
  type ToolApprovalRequest
} from './composables/useChatEventBridge'
import { useChatMessageArtifacts } from './composables/useChatMessageArtifacts'
import { usePetWindowBehavior } from './composables/usePetWindowBehavior'
import { normalizeToolName, renderToolLabel, truncateMiddle } from './utils/toolDisplay'

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

interface GenerateImageResponse {
  userMessage: Message
  assistantMessage: Message
  imageUrl: string
}

type ConversationMenuCommand = 'pin' | 'rename' | 'delete'

const MAX_INLINE_FILE_SIZE = 1_500_000
const MAX_INLINE_TEXT_CHARS = 80_000
const MAX_TOTAL_INLINE_CHARS = 140_000
const PINNED_CONVERSATION_STORAGE_KEY = 'petool.pinned-conversation-ids'

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
const useCustomWindowChrome = import.meta.env.VITE_CUSTOM_CHROME !== '0'

const inputMessage = ref('')
const newConversationTitle = ref('')
const showSettings = ref(false)
const createDialogVisible = ref(false)
const createConversationWorkspaceDirectory = ref<string | null>(null)
const pendingUploads = ref<UploadAttachment[]>([])
const workspaceRef = ref<HTMLElement | null>(null)
const messageListRef = ref<HTMLElement | null>(null)
const activeAssistantMessageIdByConversation = ref<Record<string, string>>({})
const pendingToolApproval = ref<ToolApprovalRequest | null>(null)
const resolvingToolApproval = ref(false)
const pausingStream = ref(false)
const generatingImage = ref(false)
const isWindowMaximized = ref(false)
const unlistenFns: Array<() => void> = []
const AUTO_SCROLL_BOTTOM_THRESHOLD = 72
let messageListScrollFrame: number | null = null
const appWindow = getCurrentWindow()
const {
  handleManualDrag: handleManualDragInternal,
  handleWorkspaceMouseDown: handleWorkspaceMouseDownInternal,
  setupCursorPassthrough,
  teardownCursorPassthrough
} =
  usePetWindowBehavior(workspaceRef)
const userAvatarUrl =
  'https://lh3.googleusercontent.com/aida-public/AB6AXuBYaZM97JogdW-ya3ULqGOtiyNOHmX7QgQJQ1c7qMdDxTpN__9ZBn0Jq6D5AQiHwClbXSmKaP3yFa-GzJuTHIsZ6OObIjCQ9QHApIpAuKMYIWptOHH6KVzLGp4nU5DO48mIg48o3YedtwFShv6G0Tq-ir30SVT7WgAWCksaPf_PnwnEwCx7rOimt23ZlQC3VUyfRbucQrEvpTkLIEwEwiWZ_gSWFyekl4IxXUqKEUqrS2CVHHlvuJqUmCJBLBYKUuDKiuQqkueqB3Y'
const modelOptionsBase = [
  'glm-5',
  'doubao-seed-1-6-thinking-250715',
  'MiniMax-M2.5'
]
const MODEL_LABELS: Record<string, string> = {
  'glm-5': 'GLM-5',
  'doubao-seed-1-6-thinking-250715': '豆包 Doubao Seed 1.6 Thinking',
  'minimax-m2.5': 'MiniMax M2.5'
}
const TOOL_STEP_LABELS: Record<string, string> = {
  workspace_run_command: '执行命令',
  workspace_list_directory: '浏览目录',
  workspace_read_file: '读取文件',
  workspace_write_file: '写入文件',
  workspace_search_files: '搜索文件',
  workspace_edit_file: '编辑文件',
  workspace_delete_file: '删除文件',
  workspace_move_file: '移动文件',
  workspace_copy_file: '复制文件',
  workspace_create_directory: '创建目录',
  skills_discover: '发现技能',
  skills_plan: '规划步骤',
  skills_execute: '执行技能'
}

function loadPinnedConversationIds() {
  if (typeof window === 'undefined') return []
  try {
    const raw = window.localStorage.getItem(PINNED_CONVERSATION_STORAGE_KEY)
    if (!raw) return []
    const parsed = JSON.parse(raw)
    if (!Array.isArray(parsed)) return []
    return parsed.filter((item): item is string => typeof item === 'string')
  } catch {
    return []
  }
}

function dedupeConversationIds(ids: string[]) {
  return ids.filter((id, index) => id && ids.indexOf(id) === index)
}

const pinnedConversationIds = ref<string[]>(dedupeConversationIds(loadPinnedConversationIds()))

function persistPinnedConversationIds(ids: string[]) {
  const normalized = dedupeConversationIds(ids)
  pinnedConversationIds.value = normalized
  if (typeof window === 'undefined') return
  window.localStorage.setItem(PINNED_CONVERSATION_STORAGE_KEY, JSON.stringify(normalized))
}

const activeAssistantMessageId = computed<string | null>({
  get: () => {
    const conversationId = chatStore.currentConversationId
    if (!conversationId) return null
    return activeAssistantMessageIdByConversation.value[conversationId] || null
  },
  set: (value) => {
    const conversationId = chatStore.currentConversationId
    if (!conversationId) return
    if (!value) {
      delete activeAssistantMessageIdByConversation.value[conversationId]
      return
    }
    activeAssistantMessageIdByConversation.value[conversationId] = value
  }
})

const {
  reasoningByMessage,
  toolStreamItems,
  collapseActiveReasoning,
  getReasoningEntry,
  toggleReasoning,
  isStreamingMessage,
  isRenderableMessage,
  hydrateConversationArtifacts,
  shouldShowMessageBubble,
  shouldRenderAssistantContent,
  persistToolItemsForActiveMessage,
  initializeAssistantArtifacts,
  clearAssistantArtifacts,
  getToolItemsForMessage,
  shouldShowToolProgress,
  isToolListCollapsed,
  toggleCompactToolList,
  getCompactToolSummaryTitle,
  getCompactToolSummaryCount,
  renderToolItemName,
  getToolStatusLabel,
  renderToolArguments,
  renderToolResult,
  getDisplayedMessageContent
} = useChatMessageArtifacts({
  chatStore,
  activeAssistantMessageId,
  toolStepLabels: TOOL_STEP_LABELS
})

const isCurrentConversationStreaming = computed(() =>
  chatStore.isConversationStreaming(chatStore.currentConversationId)
)

const conversationsForDisplay = computed(() => {
  const pinnedSet = new Set(pinnedConversationIds.value)
  const pinned = chatStore.conversations.filter((conversation) => pinnedSet.has(conversation.id))
  const unpinned = chatStore.conversations.filter((conversation) => !pinnedSet.has(conversation.id))
  return [...pinned, ...unpinned]
})

const conversationModelId = computed(() => {
  const source = chatStore.currentConversation?.model || configStore.config.model || modelOptionsBase[0]
  return normalizeModelId(source)
})

const activeModelId = computed(() => conversationModelId.value)

const activeModelLabel = computed(() => {
  return formatModelLabel(activeModelId.value)
})

const modelOptions = computed(() => {
  const seen = new Set<string>()
  const candidates = [activeModelId.value, conversationModelId.value, ...modelOptionsBase]
  const options: string[] = []

  for (const model of candidates) {
    const normalized = normalizeModelId(model)
    if (!normalized || seen.has(normalized)) continue
    seen.add(normalized)
    options.push(normalized)
  }

  return options
})

const toolDisplayMode = computed(() => (configStore.config.tool_display_mode === 'full' ? 'full' : 'compact'))
const isToolDisplayFull = computed(() => toolDisplayMode.value === 'full')
const shouldStickToMessageBottom = ref(true)

function getMessageListDistanceFromBottom(element: HTMLElement) {
  return element.scrollHeight - element.scrollTop - element.clientHeight
}

function updateShouldStickToMessageBottom() {
  const element = messageListRef.value
  if (!element) {
    shouldStickToMessageBottom.value = true
    return
  }
  shouldStickToMessageBottom.value = getMessageListDistanceFromBottom(element) <= AUTO_SCROLL_BOTTOM_THRESHOLD
}

function handleMessageListScroll() {
  updateShouldStickToMessageBottom()
}

function scheduleScrollMessageListToBottom(force = false) {
  if (messageListScrollFrame !== null) {
    cancelAnimationFrame(messageListScrollFrame)
  }

  messageListScrollFrame = requestAnimationFrame(() => {
    messageListScrollFrame = null
    const element = messageListRef.value
    if (!element) return
    if (!force && !shouldStickToMessageBottom.value) return
    element.scrollTop = element.scrollHeight
    shouldStickToMessageBottom.value = true
  })
}

watch(
  () => chatStore.conversations.map((conversation) => conversation.id),
  (conversationIds) => {
    const validIds = new Set(conversationIds)
    const nextPinnedIds = pinnedConversationIds.value.filter((id) => validIds.has(id))
    if (nextPinnedIds.length !== pinnedConversationIds.value.length) {
      persistPinnedConversationIds(nextPinnedIds)
    }
  },
  { immediate: true }
)

watch(
  () => chatStore.currentConversationId,
  () => {
    pausingStream.value = false
    shouldStickToMessageBottom.value = true
    scheduleScrollMessageListToBottom(true)
  },
  { flush: 'post' }
)

watch(
  () => chatStore.currentMessages.length,
  () => {
    scheduleScrollMessageListToBottom()
  },
  { flush: 'post' }
)

watch(
  () => {
    const messages = chatStore.currentMessages
    if (messages.length === 0) return ''
    const last = messages[messages.length - 1]
    return `${last.id}:${last.content.length}`
  },
  () => {
    scheduleScrollMessageListToBottom()
  },
  { flush: 'post' }
)

watch(
  toolStreamItems,
  () => {
    scheduleScrollMessageListToBottom()
  },
  { deep: true, flush: 'post' }
)

watch(
  reasoningByMessage,
  () => {
    scheduleScrollMessageListToBottom()
  },
  { deep: true, flush: 'post' }
)

watch(
  createDialogVisible,
  (visible) => {
    if (visible) return
    shouldStickToMessageBottom.value = true
    scheduleScrollMessageListToBottom(true)
  },
  { flush: 'post' }
)

watch(
  messageListRef,
  () => {
    updateShouldStickToMessageBottom()
    scheduleScrollMessageListToBottom(true)
  },
  { flush: 'post' }
)

watch(
  isCurrentConversationStreaming,
  (streaming) => {
    if (!streaming) {
      pausingStream.value = false
    }
  }
)

function normalizeWorkspaceDirectory(value: string | null | undefined) {
  const normalized = value?.trim()
  return normalized ? normalized : null
}

function getDefaultWorkspaceDirectory() {
  return normalizeWorkspaceDirectory(configStore.config.work_directory)
}

function getConversationWorkspaceDirectory(conversationId: string | null | undefined) {
  if (!conversationId) return null
  return normalizeWorkspaceDirectory(configStore.config.conversation_workspaces?.[conversationId])
}

function getEffectiveWorkspaceDirectory(conversationId: string | null | undefined) {
  return getConversationWorkspaceDirectory(conversationId) || getDefaultWorkspaceDirectory()
}

async function applyWorkspaceDirectory(directory: string | null) {
  fsStore.currentDirectory = directory
  if (!directory) {
    fsStore.files = []
    return
  }

  try {
    const rootFiles = await fsStore.scanDirectory(directory)
    fsStore.files = rootFiles
    fsStore.children[directory] = rootFiles
  } catch {
    fsStore.files = []
  }
}

async function persistConversationWorkspaceDirectory(conversationId: string, directory: string | null) {
  const currentMap = configStore.config.conversation_workspaces || {}
  const nextMap = { ...currentMap }

  if (directory) {
    nextMap[conversationId] = directory
  } else {
    delete nextMap[conversationId]
  }

  const hasChanged =
    Object.keys(currentMap).length !== Object.keys(nextMap).length ||
    Object.entries(nextMap).some(([id, path]) => currentMap[id] !== path)

  if (!hasChanged) return

  await configStore.saveConfig({
    ...configStore.config,
    conversation_workspaces: nextMap
  })
}

function handleManualDrag() {
  if (!useCustomWindowChrome) return
  handleManualDragInternal()
}

function handleWorkspaceMouseDown(event: MouseEvent) {
  if (!useCustomWindowChrome) return
  handleWorkspaceMouseDownInternal(event)
}

const recentFolders = computed(() => {
  const paths = [createConversationWorkspaceDirectory.value, fsStore.currentDirectory, configStore.config.work_directory]
    .filter((path): path is string => Boolean(path && path.trim()))
  return Array.from(new Set(paths)).slice(0, 3)
})

const canSendMessage = computed(() => {
  return inputMessage.value.trim().length > 0 || pendingUploads.value.length > 0
})

const canGenerateImagePrompt = computed(() => {
  return inputMessage.value.trim().length > 0
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
    return '为了解决当前问题，我希望从 ClawHub 下载并安装一个技能。'
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
      return `来源：${truncateMiddle(repoUrlRaw, 72)}`
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
    hydrateConversationArtifacts(first.id)
    await applyWorkspaceDirectory(getEffectiveWorkspaceDirectory(first.id))
    createDialogVisible.value = false
  } else {
    await applyWorkspaceDirectory(getDefaultWorkspaceDirectory())
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
      onStreamEnd: (conversationId) => {
        if (!conversationId) return
        if (pendingToolApproval.value?.conversationId === conversationId) {
          pendingToolApproval.value = null
          resolvingToolApproval.value = false
        }
        if (conversationId === chatStore.currentConversationId) {
          persistToolItemsForActiveMessage()
          collapseActiveReasoning()
          pausingStream.value = false
        }
        delete activeAssistantMessageIdByConversation.value[conversationId]
      }
    }))
  )

  if (useCustomWindowChrome) {
    setupCursorPassthrough()
    try {
      await syncWindowMaximizedState()
      const unlistenResize = await appWindow.listen('tauri://resize', () => {
        void syncWindowMaximizedState()
      })
      unlistenFns.push(unlistenResize)
    } catch {
      // ignore window control setup failures in non-Tauri runtime
    }

  }

  scheduleScrollMessageListToBottom(true)
})

onBeforeUnmount(() => {
  teardownCursorPassthrough()
  if (messageListScrollFrame !== null) {
    cancelAnimationFrame(messageListScrollFrame)
    messageListScrollFrame = null
  }
  for (const unlisten of unlistenFns) {
    unlisten()
  }
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

function normalizeModelId(value: string) {
  const trimmed = value.trim()
  if (!trimmed) return modelOptionsBase[0]
  if (/^glm-5$/i.test(trimmed)) return 'glm-5'
  if (/^doubao-seed-1-6-thinking-250715$/i.test(trimmed)) return 'doubao-seed-1-6-thinking-250715'
  if (/^minimax-m2\.5$/i.test(trimmed)) return 'MiniMax-M2.5'
  return trimmed
}

function formatModelLabel(value: string) {
  const normalized = normalizeModelId(value)
  return MODEL_LABELS[normalized.toLowerCase()] || normalized
}

async function handleSelectModel(model: string) {
  const conversationId = chatStore.currentConversationId
  if (!conversationId) return
  const normalizedModel = normalizeModelId(model)
  if (!normalizedModel || normalizedModel === normalizeModelId(chatStore.currentConversation?.model || '')) return

  try {
    await chatStore.updateConversationModel(conversationId, normalizedModel)
  } catch (error) {
    ElMessage.error(getErrorMessage(error, '切换模型失败'))
  }
}

function getConversationIcon(index: number) {
  const icons = ['folder', 'event_note', 'palette', 'description', 'topic', 'dashboard']
  return icons[index % icons.length]
}

function openCreateDialog() {
  newConversationTitle.value = ''
  createConversationWorkspaceDirectory.value = null
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
  createConversationWorkspaceDirectory.value = folder
}

async function handleSelectFolder() {
  try {
    const selected = await invoke<string | null>('select_folder')
    if (!selected) return
    createConversationWorkspaceDirectory.value = selected
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
  hydrateConversationArtifacts(id)
  await applyWorkspaceDirectory(getEffectiveWorkspaceDirectory(id))
  generatingImage.value = false
  toolStreamItems.value = []
  if (pendingToolApproval.value?.conversationId !== id) {
    pendingToolApproval.value = null
    resolvingToolApproval.value = false
  }
  createDialogVisible.value = false
}

function isConversationPinned(id: string) {
  return pinnedConversationIds.value.includes(id)
}

function togglePinnedConversation(id: string) {
  if (isConversationPinned(id)) {
    persistPinnedConversationIds(pinnedConversationIds.value.filter((item) => item !== id))
    return
  }
  persistPinnedConversationIds([...pinnedConversationIds.value, id])
}

async function handleRenameConversation(id: string) {
  const targetConversation = chatStore.conversations.find((item) => item.id === id)
  if (!targetConversation) return

  try {
    const promptResult = await ElMessageBox.prompt('请输入新的会话名称', '重命名', {
      confirmButtonText: '保存',
      cancelButtonText: '取消',
      inputValue: targetConversation.title,
      inputValidator: (inputValue) => (inputValue.trim().length > 0 ? true : '名称不能为空')
    })
    const nextTitle = String((promptResult as { value?: string }).value || '').trim()
    if (!nextTitle || nextTitle === targetConversation.title) return
    await chatStore.renameConversation(id, nextTitle)
  } catch (error) {
    if (error === 'cancel' || error === 'close') return
    ElMessage.error(getErrorMessage(error, '重命名会话失败'))
  }
}

async function handleConversationMenuCommand(command: ConversationMenuCommand, id: string) {
  if (command === 'pin') {
    togglePinnedConversation(id)
    return
  }
  if (command === 'rename') {
    await handleRenameConversation(id)
    return
  }
  if (command === 'delete') {
    await handleDeleteConversation(id)
  }
}

async function handleConversationMenuCommandById(id: string, command: string | number | object) {
  const normalized = String(command) as ConversationMenuCommand
  await handleConversationMenuCommand(normalized, id)
}

async function handleDeleteConversation(id: string) {
  const targetConversation = chatStore.conversations.find((item) => item.id === id)
  if (!targetConversation) return

  try {
    await ElMessageBox.confirm(
      `确认删除「${targetConversation.title}」吗？该操作不可撤销。`,
      '删除会话',
      {
        confirmButtonText: '删除',
        cancelButtonText: '取消',
        type: 'warning'
      }
    )
  } catch {
    return
  }

  const deletingCurrentConversation = chatStore.currentConversationId === id
  const fallbackConversationId = deletingCurrentConversation
    ? (conversationsForDisplay.value.find((item) => item.id !== id)?.id ?? null)
    : chatStore.currentConversationId

  try {
    await chatStore.deleteConversation(id)
    persistPinnedConversationIds(pinnedConversationIds.value.filter((item) => item !== id))
    await persistConversationWorkspaceDirectory(id, null)
    delete activeAssistantMessageIdByConversation.value[id]

    if (chatStore.conversations.length === 0) {
      chatStore.setCurrentConversation(null)
      await applyWorkspaceDirectory(getDefaultWorkspaceDirectory())
      pausingStream.value = false
      generatingImage.value = false
      toolStreamItems.value = []
      pendingToolApproval.value = null
      resolvingToolApproval.value = false
      createDialogVisible.value = true
      return
    }

    if (deletingCurrentConversation && fallbackConversationId) {
      await handleSelectConversation(fallbackConversationId)
    }
  } catch (error) {
    ElMessage.error(getErrorMessage(error, '删除会话失败'))
  }
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
  const selectedWorkspace = normalizeWorkspaceDirectory(createConversationWorkspaceDirectory.value)

  try {
    const model = configStore.config.model || 'glm-5'
    const conversation = await chatStore.createConversation(title, model)
    await persistConversationWorkspaceDirectory(conversation.id, selectedWorkspace)
    chatStore.setCurrentConversation(conversation.id)
    await chatStore.loadMessages(conversation.id)
    hydrateConversationArtifacts(conversation.id)
    await applyWorkspaceDirectory(getEffectiveWorkspaceDirectory(conversation.id))
    inputMessage.value = ''
    createConversationWorkspaceDirectory.value = null
    createDialogVisible.value = false
  } catch (error) {
    ElMessage.error(getErrorMessage(error, '创建任务失败'))
  }
}

async function sendMessage() {
  const content = inputMessage.value.trim()
  const uploads = [...pendingUploads.value]
  if ((!content && uploads.length === 0) || !chatStore.currentConversationId || isCurrentConversationStreaming.value) return

  const conversationId = chatStore.currentConversationId
  const workspaceDirectory = resolveWorkspaceDirectoryForSend(uploads)
  if (!workspaceDirectory) {
    ElMessage.warning('请先在“新冒险”选择工作区文件夹，或在设置中配置默认工作目录。')
    return
  }
  const messageContentForModel = buildMessageForModel(content, uploads, workspaceDirectory)
  const messageContentForView = buildMessageForDisplay(content, uploads)

  inputMessage.value = ''
  pendingUploads.value = []
  toolStreamItems.value = []
  if (pendingToolApproval.value?.conversationId === conversationId) {
    pendingToolApproval.value = null
  }

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
  initializeAssistantArtifacts(assistantMsg.id)

  activeAssistantMessageIdByConversation.value[conversationId] = assistantMsg.id
  chatStore.setConversationStreaming(conversationId, true)
  pausingStream.value = false

  try {
    await invoke('stream_message', {
      conversationId,
      content: messageContentForModel,
      workspaceDirectory
    })
  } catch (error) {
    chatStore.setConversationStreaming(conversationId, false)
    removePendingAssistantMessage(conversationId, assistantMsg.id)
    delete activeAssistantMessageIdByConversation.value[conversationId]
    clearAssistantArtifacts(assistantMsg.id)
    if (pendingToolApproval.value?.conversationId === conversationId) {
      pendingToolApproval.value = null
      resolvingToolApproval.value = false
    }
    if (chatStore.currentConversationId === conversationId) {
      pausingStream.value = false
      toolStreamItems.value = []
      inputMessage.value = content
      pendingUploads.value = uploads
    }
    ElMessage.error(getErrorMessage(error, '发送消息失败'))
  }
}

async function pauseStream() {
  const conversationId = chatStore.currentConversationId
  if (!conversationId || !isCurrentConversationStreaming.value || pausingStream.value) return

  pausingStream.value = true
  try {
    await invoke('stop_stream', { conversationId })
  } catch (error) {
    pausingStream.value = false
    ElMessage.error(getErrorMessage(error, '暂停失败'))
  }
}

async function generateImage() {
  const prompt = inputMessage.value.trim()
  const conversationId = chatStore.currentConversationId
  if (!prompt || !conversationId || isCurrentConversationStreaming.value || generatingImage.value) return

  generatingImage.value = true
  inputMessage.value = ''

  try {
    const result = await invoke<GenerateImageResponse>('generate_image', {
      conversationId,
      prompt
    })
    chatStore.addMessage(conversationId, result.userMessage)
    chatStore.addMessage(conversationId, result.assistantMessage)
  } catch (error) {
    inputMessage.value = prompt
    ElMessage.error(getErrorMessage(error, '文生图失败'))
  } finally {
    generatingImage.value = false
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

function buildMessageForModel(content: string, uploads: UploadAttachment[], workspaceDirectory: string) {
  if (uploads.length === 0) return content.trim()

  let remainingInlineBudget = MAX_TOTAL_INLINE_CHARS
  const lines: string[] = []
  lines.push('【用户上传文件】')
  lines.push(`当前工作区: ${workspaceDirectory}`)
  lines.push('重要限制: 若需要创建/修改文件，只能在当前工作区内操作。')

  for (let i = 0; i < uploads.length; i += 1) {
    const item = uploads[i]
    const insideWorkspace = isPathInside(workspaceDirectory, item.path)
    lines.push(`${i + 1}. 文件名: ${item.name}`)
    lines.push(`   路径: ${item.path}`)
    lines.push(`   大小: ${formatBytes(item.size)}`)
    lines.push(`   说明: ${item.note}`)
    lines.push(`   工作区内: ${insideWorkspace ? '是' : '否'}`)

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
      if (insideWorkspace) {
        lines.push('   内容片段: 未内联，请通过文件路径读取。')
      } else {
        lines.push('   内容片段: 未内联，且该路径在工作区外，无法通过工作区工具读取。')
      }
    }
  }

  lines.push('请优先基于上述附件内容完成分析；若内容未内联，请仅在工作区内使用可用工具读取路径。')

  const userText = content.trim() || '请分析这些文件，并给出清晰结论。'
  return `${userText}\n\n${lines.join('\n')}`
}

function resolveWorkspaceDirectoryForSend(uploads: UploadAttachment[]) {
  const conversationId = chatStore.currentConversationId
  const configuredWorkspace = getEffectiveWorkspaceDirectory(conversationId)
  if (!configuredWorkspace) return null
  if (uploads.length === 0) return configuredWorkspace
  return configuredWorkspace
}

function isPathInside(basePath: string, targetPath: string) {
  const base = normalizePathForCompare(basePath)
  const target = normalizePathForCompare(targetPath)
  return target === base || target.startsWith(`${base}/`)
}

function normalizePathForCompare(value: string) {
  return value.replace(/\\/g, '/').replace(/\/+$/, '').toLowerCase()
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
</script>

<style scoped src="./styles/app-shell.css"></style>



