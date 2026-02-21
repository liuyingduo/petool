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
        <Sidebar
          :conversations="conversationsForDisplay"
          :active-conversation-id="chatStore.currentConversationId"
          :pinned-ids="pinnedConversationIdSet"
          :streaming-status-map="streamingStatusMap"
          :user-avatar="displayAvatar"
          :user-name="displayName"
          :user-plan="displayPlan"
          :memo-deps="sidebarListMemoDeps"
          @create="openCreateDialog"
          @select="handleSelectConversation"
          @command="handleConversationMenuCommandById"
          @account="openAccountCenter"
          @settings="openSettingsCenter"
        />

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

          <ChatTimeline
            v-else
            ref="messageListRef"
            v-memo="messageListMemoDeps"
            :turns="timelineTurnsForDisplay"
            :groups-by-turn-id="turnToolExecutionGroupsByTurnId"
            :is-tool-display-full="isToolDisplayFull"
            :is-legacy-timeline="chatStore.currentTimelineLegacy"
            :should-show-standalone-typing-bubble="shouldShowStandaloneTypingBubble"
            :user-name="displayName"
            :user-avatar="displayAvatar"
            @scroll="handleMessageListScroll"
          />

          <TaskMonitor
            v-if="!createDialogVisible"
            :collapsed="taskMonitorCollapsed"
            :todos="monitorTodos"
            :artifacts="monitorArtifacts"
            :skills="monitorSkills"
            :current-directory="fsStore.currentDirectory"
            @toggle="toggleTaskMonitor"
          />

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

        <ChatInput
          v-model="userInput"
          :disabled="createDialogVisible || !chatStore.currentConversationId"
          :is-streaming="isCurrentConversationStreaming"
          :is-pausing="pausingStream"
          :uploads="pendingUploads"
          :active-model-id="activeModelId"
          :active-model-label="activeModelLabel"
          :model-options="modelOptions"
          :format-model-label="formatModelLabel"
          @send-message="sendMessage()"
          @pause-stream="pauseStream"
          @select-upload-files="handleSelectUploadFiles"
          @remove-upload="removeUpload"
          @select-model="handleSelectModel"
        />
        </div>
        </section>
      </div>
    </main>

  </div>
</template>

<script setup lang="ts">
import { computed, onActivated, onBeforeUnmount, onDeactivated, onMounted, ref, watch } from 'vue'
import { useRouter } from 'vue-router'
import { ElMessage, ElMessageBox } from 'element-plus'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { open as openDialog } from '@tauri-apps/plugin-dialog'
import { useChatStore } from './stores/chat'
import { useConfigStore } from './stores/config'
import { useFilesystemStore } from './stores/filesystem'
import TaskMonitor from '@/components/chat/TaskMonitor.vue'
import Sidebar from '@/components/chat/Sidebar.vue'
import ChatInput from '@/components/chat/ChatInput.vue'
import ChatTimeline from '@/components/chat/ChatTimeline.vue'
import {
  registerChatEventListeners,
  type ToolApprovalRequest
} from './composables/useChatEventBridge'
import { usePetWindowBehavior } from './composables/usePetWindowBehavior'
import { useDisplayProfile } from './composables/useDisplayProfile'
import { normalizeToolName, renderToolLabel, truncateMiddle } from './utils/toolDisplay'

interface UploadAttachment {
  id: string
  path: string
  name: string
  extension: string
  size: number
}

interface PathInfo {
  name: string
  path: string
  is_dir: boolean
  size?: number
  extension?: string
}

type ConversationMenuCommand = 'pin' | 'rename' | 'delete'

const PINNED_CONVERSATION_STORAGE_KEY = 'petool.pinned-conversation-ids'

const chatStore = useChatStore()
const configStore = useConfigStore()
const fsStore = useFilesystemStore()
const router = useRouter()
const { displayName, displayAvatar, displayPlan, loadDisplayProfile } = useDisplayProfile()
const useCustomWindowChrome = import.meta.env.VITE_CUSTOM_CHROME !== '0'

const newConversationTitle = ref('')
const createDialogVisible = ref(false)
const createConversationWorkspaceDirectory = ref<string | null>(null)
const pendingUploads = ref<UploadAttachment[]>([])
const workspaceRef = ref<HTMLElement | null>(null)
const messageListRef = ref<HTMLElement | null>(null)
const pendingToolApproval = ref<ToolApprovalRequest | null>(null)
const resolvingToolApproval = ref(false)
const pausingStream = ref(false)

const isWindowMaximized = ref(false)
const handlingClosePrompt = ref(false)
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

function openSettingsCenter() {
  void router.push('/settings/general')
}

function openAccountCenter() {
  void router.push('/account/profile')
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
const pinnedConversationIdSet = computed(() => new Set(pinnedConversationIds.value))

function persistPinnedConversationIds(ids: string[]) {
  const normalized = dedupeConversationIds(ids)
  pinnedConversationIds.value = normalized
  if (typeof window === 'undefined') return
  window.localStorage.setItem(PINNED_CONVERSATION_STORAGE_KEY, JSON.stringify(normalized))
}

const isCurrentConversationStreaming = computed(() =>
  chatStore.isConversationStreaming(chatStore.currentConversationId)
)

const shouldShowStandaloneTypingBubble = computed(() => {
  if (!isCurrentConversationStreaming.value) return false
  const events = chatStore.currentTimeline
  if (events.length === 0) return true

  const currentTurnId = events[events.length - 1].turn_id
  const hasAssistantEventsInCurrentTurn = events.some((event) => {
    return event.turn_id === currentTurnId && event.event_type !== 'user_message'
  })

  return !hasAssistantEventsInCurrentTurn
})

const conversationsForDisplay = computed(() => {
  const pinnedSet = pinnedConversationIdSet.value
  const pinned = chatStore.conversations.filter((conversation) => pinnedSet.has(conversation.id))
  const unpinned = chatStore.conversations.filter((conversation) => !pinnedSet.has(conversation.id))
  return [...pinned, ...unpinned]
})

const conversationStreamingRenderToken = computed(() =>
  conversationsForDisplay.value
    .map((conversation) => `${conversation.id}:${chatStore.isConversationStreaming(conversation.id) ? 1 : 0}`)
    .join('|')
)

const streamingStatusMap = computed(() => {
  const map: Record<string, boolean> = {}
  for (const conv of conversationsForDisplay.value) {
    map[conv.id] = chatStore.isConversationStreaming(conv.id)
  }
  return map
})

const sidebarListMemoDeps = computed(() => [
  conversationsForDisplay.value,
  chatStore.currentConversationId || '',
  conversationStreamingRenderToken.value
])

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
const timelineReasoningCollapsedByEventId = ref<Record<string, boolean>>({})
const timelineReasoningCollapsedVersion = ref(0)
const timelineToolCompactDetailCache = new Map<string, string>()
const compactToolExecutionCollapsedByGroupKey = ref<Record<string, boolean>>({})
const compactToolExecutionVersion = ref(0)
const taskMonitorCollapsed = ref(false)

function toggleTaskMonitor() {
  taskMonitorCollapsed.value = !taskMonitorCollapsed.value
}

import {
  type CompactToolExecutionStep,
  type CompactToolExecutionGroup,
  type MonitorTodoItem,
  type MonitorArtifactItem,
  type TimelineTurnDisplay,
  buildCompactToolExecutionGroups,
  mapToolNameToMonitorSkill,
  isArtifactToolName,
  resolveArtifactAction,
  getPathName
} from '@/utils/timeline-formatter'

const timelineTurnsForDisplay = computed<TimelineTurnDisplay[]>(() => {
  const turns: TimelineTurnDisplay[] = []
  const byTurn = new Map<string, TimelineTurnDisplay>()

  for (const event of chatStore.currentTimeline) {
    let turn = byTurn.get(event.turn_id)
    if (!turn) {
      turn = {
        turnId: event.turn_id,
        userText: '',
        userCreatedAt: event.created_at,
        assistantCreatedAt: '',
        assistantEvents: []
      }
      byTurn.set(event.turn_id, turn)
      turns.push(turn)
    }

    if (event.event_type === 'user_message') {
      const content =
        typeof event.payload.content === 'string'
          ? event.payload.content
          : String(event.payload.content ?? '')
      turn.userText = content
      turn.userCreatedAt = event.created_at
      continue
    }

    turn.assistantEvents.push(event)
    if (!turn.assistantCreatedAt) {
      turn.assistantCreatedAt = event.created_at
    }
  }

  return turns
})

const timelineRenderToken = computed(() => {
  const events = chatStore.currentTimeline
  if (events.length === 0) return 'empty'
  const last = events[events.length - 1]
  return `${events.length}:${last.id}:${last.seq}`
})

const messageListMemoDeps = computed(() => [
  chatStore.currentConversationId || '',
  chatStore.currentTimelineLegacy ? 1 : 0,
  timelineRenderToken.value,
  isToolDisplayFull.value ? 1 : 0,
  shouldShowStandaloneTypingBubble.value ? 1 : 0,
  timelineReasoningCollapsedVersion.value,
  compactToolExecutionVersion.value
])

const turnToolExecutionGroupsByTurnId = computed<Record<string, CompactToolExecutionGroup[]>>(() => {
  const map: Record<string, CompactToolExecutionGroup[]> = {}
  for (const turn of timelineTurnsForDisplay.value) {
    map[turn.turnId] = buildCompactToolExecutionGroups(turn.turnId, turn.assistantEvents)
  }
  return map
})

const turnToolExecutionsByTurnId = computed<Record<string, CompactToolExecutionStep[]>>(() => {
  const map: Record<string, CompactToolExecutionStep[]> = {}
  for (const turn of timelineTurnsForDisplay.value) {
    const groups = turnToolExecutionGroupsByTurnId.value[turn.turnId] || []
    map[turn.turnId] = groups.flatMap((group) => group.steps)
  }
  return map
})

const latestMonitorTurn = computed<TimelineTurnDisplay | null>(() => {
  const turns = timelineTurnsForDisplay.value
  for (let i = turns.length - 1; i >= 0; i -= 1) {
    const turn = turns[i]
    if (turn.assistantEvents.length > 0) return turn
  }
  return null
})

const monitorToolExecutions = computed(() => {
  const turn = latestMonitorTurn.value
  if (!turn) return []
  return turnToolExecutionsByTurnId.value[turn.turnId] || []
})

const monitorTodos = computed<MonitorTodoItem[]>(() => {
  const rows: MonitorTodoItem[] = []
  for (let i = 0; i < monitorToolExecutions.value.length; i += 1) {
    const step = monitorToolExecutions.value[i]
    rows.push({
      id: `${step.id}-${i}`,
      label: step.detail || step.title,
      status: step.status
    })
  }
  return rows
})

const monitorArtifacts = computed<MonitorArtifactItem[]>(() => {
  const rows: MonitorArtifactItem[] = []
  const seen = new Set<string>()
  for (let i = 0; i < monitorToolExecutions.value.length; i += 1) {
    const step = monitorToolExecutions.value[i]
    if (!step.artifactPath || !isArtifactToolName(step.toolName)) continue
    if (seen.has(step.artifactPath)) continue
    seen.add(step.artifactPath)
    rows.push({
      id: `${step.id}-${step.artifactPath}`,
      name: getPathName(step.artifactPath) || step.artifactPath,
      path: truncateMiddle(step.artifactPath, 46),
      action: resolveArtifactAction(step.toolName),
      status: step.status
    })
  }
  return rows
})

const monitorSkills = computed(() => {
  const labels: string[] = []
  const seen = new Set<string>()
  for (const step of monitorToolExecutions.value) {
    const label = mapToolNameToMonitorSkill(step.toolName)
    if (!label || seen.has(label)) continue
    seen.add(label)
    labels.push(label)
  }
  return labels
})

const userInput = ref('')

function getComposerText() {
  return userInput.value
}

function setComposerText(val: string) {
  userInput.value = val
}

function getMessageListDistanceFromBottom(element: HTMLElement) {
  return element.scrollHeight - element.scrollTop - element.clientHeight
}

function updateShouldStickToMessageBottom() {
  const element = messageListRef.value
  if (!element) {
    shouldStickToMessageBottom.value = true
    return
  }
  
  if (isCurrentConversationStreaming.value) {
    shouldStickToMessageBottom.value = true
    return
  }

  shouldStickToMessageBottom.value = getMessageListDistanceFromBottom(element) <= AUTO_SCROLL_BOTTOM_THRESHOLD
}

function handleMessageListScroll(element?: HTMLElement) {
  if (element && 'scrollHeight' in element) {
    shouldStickToMessageBottom.value = getMessageListDistanceFromBottom(element) <= AUTO_SCROLL_BOTTOM_THRESHOLD
  } else {
    updateShouldStickToMessageBottom()
  }
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
    timelineReasoningCollapsedByEventId.value = {}
    timelineReasoningCollapsedVersion.value += 1
    timelineToolCompactDetailCache.clear()
    compactToolExecutionCollapsedByGroupKey.value = {}
    compactToolExecutionVersion.value += 1
    shouldStickToMessageBottom.value = true
    scheduleScrollMessageListToBottom(true)
  },
  { flush: 'post' }
)

watch(
  () => chatStore.currentTimeline.length,
  () => {
    scheduleScrollMessageListToBottom()
  },
  { flush: 'post' }
)

watch(
  () => {
    const events = chatStore.currentTimeline
    if (events.length === 0) return ''
    const last = events[events.length - 1]
    return `${last.id}:${last.seq}`
  },
  () => {
    scheduleScrollMessageListToBottom()
  },
  { flush: 'post' }
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
  if (toolName === 'bash') {
    return '为了完成你的请求，我需要运行一条本地命令。'
  }
  if (toolName === 'desktop') {
    return '为了操作 Windows 图形界面，我需要执行一个桌面自动化动作。'
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
  if (toolName === 'bash' && typeof args.command === 'string') {
    return `命令：${truncateMiddle(args.command, 72)}`
  }
  if (toolName === 'desktop') {
    const action = typeof args.action === 'string' ? args.action.trim() : ''
    const params = args.params && typeof args.params === 'object' ? args.params as Record<string, unknown> : null
    if (action) {
      if (params && typeof params.path === 'string') {
        return `动作：${action} · 路径：${truncateMiddle(params.path, 72)}`
      }
      if (params && typeof params.command === 'string') {
        return `动作：${action} · 命令：${truncateMiddle(params.command, 72)}`
      }
      if (params && typeof params.url === 'string') {
        return `动作：${action} · 目标：${truncateMiddle(params.url, 72)}`
      }
      return `动作：${action}`
    }
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
  const bootTasks: Array<Promise<unknown>> = []
  bootTasks.push(loadDisplayProfile())
  if (!chatStore.conversationsLoaded) {
    bootTasks.push(chatStore.loadConversations())
  }
  if (!configStore.loaded) {
    bootTasks.push(configStore.loadConfig())
  }
  if (bootTasks.length > 0) {
    await Promise.all(bootTasks)
  }

  if (chatStore.conversations.length > 0) {
    const currentId = chatStore.currentConversationId
    const hasCurrent = Boolean(currentId) && chatStore.conversations.some((item) => item.id === currentId)
    const targetConversationId = hasCurrent ? String(currentId) : chatStore.conversations[0].id

    chatStore.setCurrentConversation(targetConversationId)
    if (!chatStore.isTimelineLoaded(targetConversationId)) {
      await chatStore.loadTimeline(targetConversationId)
    }
    await applyWorkspaceDirectory(getEffectiveWorkspaceDirectory(targetConversationId))
    createDialogVisible.value = false
  } else {
    await applyWorkspaceDirectory(getDefaultWorkspaceDirectory())
    createDialogVisible.value = true
  }

  unlistenFns.push(
    ...(await registerChatEventListeners({
      chatStore,
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
          pausingStream.value = false
        }
      }
    }))
  )
  unlistenFns.push(
    await listen('app-close-requested', () => {
      void handleClose()
    })
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

onActivated(() => {
  if (useCustomWindowChrome) {
    setupCursorPassthrough()
    void syncWindowMaximizedState()
  }
  scheduleScrollMessageListToBottom(true)
})

onDeactivated(() => {
  teardownCursorPassthrough()
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
  if (handlingClosePrompt.value) return
  const behavior = configStore.config.automation?.close_behavior || 'ask'

  const minimizeToTray = async () => {
    try {
      await appWindow.hide()
    } catch {
      // ignore unsupported runtime
    }
  }

  const exitApp = async () => {
    try {
      await invoke('app_exit_now')
    } catch {
      // ignore unsupported runtime
    }
  }

  if (behavior === 'minimize_to_tray') {
    await minimizeToTray()
    return
  }
  if (behavior === 'exit') {
    await exitApp()
    return
  }

  handlingClosePrompt.value = true
  try {
    await ElMessageBox.confirm(
      '选择关闭方式：最小化到托盘继续运行，或直接退出应用。',
      '关闭 PETool',
      {
        confirmButtonText: '退出应用',
        cancelButtonText: '最小化到托盘',
        distinguishCancelAndClose: true,
        type: 'warning'
      }
    )
    await exitApp()
  } catch (error) {
    if (error === 'cancel' || error === 'close') {
      await minimizeToTray()
    }
  } finally {
    handlingClosePrompt.value = false
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
  await chatStore.loadTimeline(id)
  await applyWorkspaceDirectory(getEffectiveWorkspaceDirectory(id))

  if (pendingToolApproval.value?.conversationId !== id) {
    pendingToolApproval.value = null
    resolvingToolApproval.value = false
  }
  createDialogVisible.value = false
}

function isConversationPinned(id: string) {
  return pinnedConversationIdSet.value.has(id)
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

    if (chatStore.conversations.length === 0) {
      chatStore.setCurrentConversation(null)
      await applyWorkspaceDirectory(getDefaultWorkspaceDirectory())
      pausingStream.value = false

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
    await chatStore.loadTimeline(conversation.id)
    await applyWorkspaceDirectory(getEffectiveWorkspaceDirectory(conversation.id))
    setComposerText('')
    createConversationWorkspaceDirectory.value = null
    createDialogVisible.value = false
  } catch (error) {
    ElMessage.error(getErrorMessage(error, '创建任务失败'))
  }
}

async function sendMessage() {
  const rawContent = getComposerText().trim()
  const uploads = [...pendingUploads.value]
  if ((!rawContent && uploads.length === 0) || !chatStore.currentConversationId || isCurrentConversationStreaming.value) return

  const conversationId = chatStore.currentConversationId
  const workspaceDirectory = resolveWorkspaceDirectoryForSend()
  if (!workspaceDirectory) {
    ElMessage.warning('请先在“新冒险”选择工作区文件夹，或在设置中配置默认工作目录。')
    return
  }
  const contentForModel = rawContent || '请分析这些文件，并给出清晰结论。'

  setComposerText('')
  pendingUploads.value = []
  if (pendingToolApproval.value?.conversationId === conversationId) {
    pendingToolApproval.value = null
  }
  chatStore.setConversationStreaming(conversationId, true)
  pausingStream.value = false
  shouldStickToMessageBottom.value = true
  scheduleScrollMessageListToBottom(true)

  try {
    await invoke('stream_message', {
      conversationId,
      content: contentForModel,
      workspaceDirectory,
      attachments: uploads.map((item) => toUploadedAttachmentInput(item))
    })
  } catch (error) {
    chatStore.setConversationStreaming(conversationId, false)
    if (pendingToolApproval.value?.conversationId === conversationId) {
      pendingToolApproval.value = null
      resolvingToolApproval.value = false
    }
    if (chatStore.currentConversationId === conversationId) {
      pausingStream.value = false
      setComposerText(rawContent)
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
  return {
    id: `${Date.now()}-${Math.random().toString(16).slice(2, 8)}`,
    path,
    name: getPathName(path) || path,
    extension,
    size
  }
}

interface UploadedAttachmentInput {
  path: string
  name: string
  size: number
  extension: string
}

function toUploadedAttachmentInput(item: UploadAttachment): UploadedAttachmentInput {
  return {
    path: item.path,
    name: item.name,
    size: item.size,
    extension: item.extension
  }
}

function resolveWorkspaceDirectoryForSend() {
  const conversationId = chatStore.currentConversationId
  const configuredWorkspace = getEffectiveWorkspaceDirectory(conversationId)
  if (!configuredWorkspace) return null
  return configuredWorkspace
}

function getPathExtension(input: string) {
  const name = getPathName(input).toLowerCase()
  const dot = name.lastIndexOf('.')
  if (dot < 0 || dot === name.length - 1) return ''
  return name.slice(dot + 1)
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


</script>

<style scoped src="./styles/app-shell.css"></style>
