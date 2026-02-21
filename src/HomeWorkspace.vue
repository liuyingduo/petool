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

        <div class="conversation-list no-scrollbar" v-memo="sidebarListMemoDeps">
          <div
            v-for="conv in conversationsForDisplay"
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
          <button
            class="sidebar-user-card account-entry-btn"
            type="button"
            title="账户中心"
            @click="openAccountCenter"
          >
            <div class="sidebar-user-meta">
              <div class="sidebar-avatar">
                <img :src="displayAvatar" alt="User Avatar" />
              </div>
              <div class="sidebar-user-text">
                <span class="name">{{ displayName }}</span>
                <span class="plan">{{ displayPlan }}</span>
              </div>
            </div>
          </button>
          <button class="sidebar-settings-btn" @click="openSettingsCenter" title="系统设置">
            <span class="material-icons-round">settings</span>
          </button>
        </div>
        </aside>

        <section class="chat-wrap">
          <div class="chat-body" :class="{ creating: createDialogVisible, 'monitor-open': !taskMonitorCollapsed }">
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
            v-memo="messageListMemoDeps"
            @click="handleMarkdownLinkClick"
            @scroll.passive="handleMessageListScroll"
          >
            <div v-if="chatStore.currentTimelineLegacy" class="empty-tip">Legacy 会话：按近似顺序回放</div>
            <template v-for="turn in timelineTurnsForDisplay" :key="turn.turnId">
              <div v-if="turn.userText" class="message-row user">
                <div class="message-meta user-meta">
                  <span class="time">{{ formatTime(turn.userCreatedAt) }}</span>
                  <span class="name">{{ displayName }}</span>
                  <div class="message-avatar">
                    <img class="avatar-img" :src="displayAvatar" alt="User Avatar" />
                  </div>
                </div>
                <div class="bubble">
                  <div v-html="renderMarkdown(turn.userText)"></div>
                </div>
              </div>

              <div v-if="turn.assistantEvents.length > 0" class="message-row assistant">
                <div class="message-meta">
                  <span class="name">Petool</span>
                  <span class="time">{{ formatTime(turn.assistantCreatedAt) }}</span>
                </div>
                <div class="bubble">
                  <div
                    v-for="event in turn.assistantEvents"
                    :key="event.id"
                    class="timeline-event"
                  >
                    <template v-if="event.event_type === 'assistant_reasoning'">
                      <div class="reasoning">
                        <button class="reasoning-toggle" @click="toggleTimelineReasoning(event.id)">
                          <span>思考过程</span>
                          <span class="reasoning-state">{{ isTimelineReasoningCollapsed(event.id) ? '已折叠' : '展开中' }}</span>
                          <span class="material-icons-round">
                            {{ isTimelineReasoningCollapsed(event.id) ? 'expand_more' : 'expand_less' }}
                          </span>
                        </button>
                        <div v-show="!isTimelineReasoningCollapsed(event.id)" class="reasoning-content">
                          {{ getTimelineReasoningText(event) }}
                        </div>
                      </div>
                    </template>

                    <template v-else-if="isTimelineToolEvent(event)">
                      <div v-if="isToolDisplayFull" class="tool-progress">
                        <div class="tool-list">
                          <div class="tool-item" :class="event.event_type === 'assistant_tool_call' ? 'running' : getTimelineToolResultStatus(event)">
                            <div class="tool-title">{{ getTimelineToolName(event) }}</div>
                            <div v-if="getTimelineToolDisplayText(event)" class="tool-text">
                              <span class="tool-text-label">{{ event.event_type === 'assistant_tool_call' ? '参数' : '结果' }}</span>
                              <pre class="tool-code">{{ getTimelineToolDisplayText(event) }}</pre>
                            </div>
                          </div>
                        </div>
                      </div>
                      <div
                        v-else-if="shouldRenderCompactToolSummary(turn.turnId, event.id)"
                        class="tool-compact-batch"
                      >
                        <button class="tool-batch-toggle" @click="toggleToolExecutionGroup(turn.turnId, event.id)">
                          <span class="material-icons-round">terminal</span>
                          <span class="tool-batch-title">查看执行工具</span>
                          <span class="tool-batch-count">{{ getToolExecutionGroupSteps(turn.turnId, event.id).length }} step</span>
                          <span class="material-icons-round">
                            {{ isToolExecutionGroupCollapsed(turn.turnId, event.id) ? 'expand_more' : 'expand_less' }}
                          </span>
                        </button>
                        <div v-show="!isToolExecutionGroupCollapsed(turn.turnId, event.id)" class="tool-batch-list">
                          <div
                            v-for="step in getToolExecutionGroupSteps(turn.turnId, event.id)"
                            :key="step.id"
                            class="tool-batch-item"
                            :class="step.status"
                          >
                            <span class="status-indicator" :class="step.status" aria-hidden="true">
                              <span v-if="step.status === 'running'" class="status-spinner"></span>
                              <span v-else-if="step.status === 'done'" class="material-icons-round">check_circle</span>
                              <span v-else class="material-icons-round">cancel</span>
                            </span>
                            <div class="tool-batch-main">
                              <div class="tool-batch-line">
                                <span class="tool-batch-name">{{ step.title }}</span>
                                <span class="tool-batch-status">{{ formatToolStepStatus(step.status) }}</span>
                              </div>
                              <div v-if="step.detail" class="tool-batch-detail">{{ step.detail }}</div>
                            </div>
                          </div>
                        </div>
                      </div>
                    </template>

                    <template v-else-if="event.event_type === 'assistant_text'">
                      <div v-html="renderMarkdown(getTimelineText(event))"></div>
                    </template>
                  </div>
                </div>
              </div>
            </template>

            <div v-if="shouldShowStandaloneTypingBubble" class="message-row assistant">
              <div class="message-meta">
                <span class="name">Petool</span>
                <span class="time">{{ formatTime(new Date().toISOString()) }}</span>
              </div>
              <div class="bubble">
                <div class="typing"><span></span><span></span><span></span></div>
              </div>
            </div>
          </div>

          <div
            v-if="!createDialogVisible"
            class="task-monitor-shell"
            :class="{ collapsed: taskMonitorCollapsed }"
          >
            <button class="task-monitor-toggle" type="button" @click="toggleTaskMonitor">
              <span class="material-icons-round">
                {{ taskMonitorCollapsed ? 'keyboard_arrow_left' : 'keyboard_arrow_right' }}
              </span>
            </button>
            <aside class="task-monitor-card">
              <div class="task-monitor-title">Task Monitor</div>

              <section class="task-monitor-section">
                <button class="task-monitor-section-head" type="button" @click="toggleMonitorSection('todos')">
                  <span>Todos</span>
                  <span class="material-icons-round">{{ monitorSectionsOpen.todos ? 'expand_less' : 'expand_more' }}</span>
                </button>
                <div v-show="monitorSectionsOpen.todos" class="task-monitor-section-body">
                  <div v-if="monitorTodos.length === 0" class="task-monitor-empty">等待工具执行...</div>
                  <div v-for="todo in monitorTodos" :key="todo.id" class="task-monitor-row">
                    <span class="status-indicator" :class="todo.status" aria-hidden="true">
                      <span v-if="todo.status === 'running'" class="status-spinner"></span>
                      <span v-else-if="todo.status === 'done'" class="material-icons-round">check_circle</span>
                      <span v-else class="material-icons-round">cancel</span>
                    </span>
                    <span class="task-monitor-row-label">{{ todo.label }}</span>
                  </div>
                </div>
              </section>

              <section class="task-monitor-section">
                <button class="task-monitor-section-head" type="button" @click="toggleMonitorSection('artifacts')">
                  <span>Artifacts</span>
                  <span class="material-icons-round">{{ monitorSectionsOpen.artifacts ? 'expand_less' : 'expand_more' }}</span>
                </button>
                <div v-show="monitorSectionsOpen.artifacts" class="task-monitor-section-body">
                  <div v-if="monitorArtifacts.length === 0" class="task-monitor-empty">
                    {{ fsStore.currentDirectory ? truncateMiddle(fsStore.currentDirectory, 38) : 'Default workspace' }}
                  </div>
                  <div v-for="artifact in monitorArtifacts" :key="artifact.id" class="task-monitor-row artifact">
                    <span class="status-indicator" :class="artifact.status" aria-hidden="true">
                      <span v-if="artifact.status === 'running'" class="status-spinner"></span>
                      <span v-else-if="artifact.status === 'done'" class="material-icons-round">check_circle</span>
                      <span v-else class="material-icons-round">cancel</span>
                    </span>
                    <div class="task-monitor-artifact-content">
                      <div class="task-monitor-row-label">{{ artifact.name }}</div>
                      <div class="task-monitor-row-sub">{{ artifact.action }} · {{ artifact.path }}</div>
                    </div>
                  </div>
                </div>
              </section>

              <section class="task-monitor-section">
                <button class="task-monitor-section-head" type="button" @click="toggleMonitorSection('skills')">
                  <span>Skills & MCP</span>
                  <span class="material-icons-round">{{ monitorSectionsOpen.skills ? 'expand_less' : 'expand_more' }}</span>
                </button>
                <div v-show="monitorSectionsOpen.skills" class="task-monitor-section-body">
                  <div v-if="monitorSkills.length === 0" class="task-monitor-empty">暂无</div>
                  <div v-for="skill in monitorSkills" :key="skill" class="task-monitor-row skill">
                    <span class="material-icons-round">api</span>
                    <span class="task-monitor-row-label">{{ skill }}</span>
                  </div>
                </div>
              </section>
            </aside>
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
              <span class="material-icons-round">{{ uploadIcon(item.extension) }}</span>
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

          <input
            ref="composerInputRef"
            type="text"
            placeholder="想让我做什么？"
            :disabled="createDialogVisible || !chatStore.currentConversationId || isCurrentConversationStreaming"
            spellcheck="false"
            autocomplete="off"
            @keydown.enter.prevent="handleComposerEnter"
          />
          <button
            class="send-btn"
            :disabled="
              createDialogVisible ||
              !chatStore.currentConversationId ||
              (isCurrentConversationStreaming ? pausingStream : false)
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

  </div>
</template>

<script setup lang="ts">
import { computed, onActivated, onBeforeUnmount, onDeactivated, onMounted, ref, watch } from 'vue'
import { useRouter } from 'vue-router'
import { marked } from 'marked'
import { ElMessage, ElMessageBox } from 'element-plus'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { open as openDialog } from '@tauri-apps/plugin-dialog'
import { open as openExternal } from '@tauri-apps/plugin-shell'
import { useChatStore, type TimelineEvent } from './stores/chat'
import { useConfigStore } from './stores/config'
import { useFilesystemStore } from './stores/filesystem'
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
const MARKDOWN_CACHE_MAX_ENTRIES = 300
const TOOL_DETAIL_CACHE_MAX_ENTRIES = 800

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
const composerInputRef = ref<HTMLInputElement | null>(null)
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
const markdownCache = new Map<string, string>()
const timelineToolCompactDetailCache = new Map<string, string>()
const compactToolExecutionCollapsedByGroupKey = ref<Record<string, boolean>>({})
const compactToolExecutionVersion = ref(0)
const taskMonitorCollapsed = ref(false)
const monitorSectionsOpen = ref({
  todos: true,
  artifacts: true,
  skills: true
})

type ToolStepStatus = 'running' | 'done' | 'error'

interface CompactToolExecutionStep {
  id: string
  toolName: string
  title: string
  detail: string
  status: ToolStepStatus
  artifactPath: string
}

interface CompactToolExecutionGroup {
  key: string
  firstEventId: string
  steps: CompactToolExecutionStep[]
}

interface MonitorTodoItem {
  id: string
  label: string
  status: ToolStepStatus
}

interface MonitorArtifactItem {
  id: string
  name: string
  path: string
  action: string
  status: ToolStepStatus
}

interface TimelineTurnDisplay {
  turnId: string
  userText: string
  userCreatedAt: string
  assistantCreatedAt: string
  assistantEvents: TimelineEvent[]
}

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

function getTimelinePayloadValue(event: TimelineEvent, key: string) {
  const payload = event.payload || {}
  return payload[key]
}

function getTimelineText(event: TimelineEvent) {
  const value = getTimelinePayloadValue(event, 'text')
  return typeof value === 'string' ? value : String(value || '')
}

function getTimelineReasoningText(event: TimelineEvent) {
  return getTimelineText(event)
}

function getTimelineToolName(event: TimelineEvent) {
  const name = getTimelinePayloadValue(event, 'name')
  if (typeof name === 'string' && name.trim()) return renderToolLabel(name)
  return '工具执行'
}

function getTimelineToolArguments(event: TimelineEvent) {
  const raw = getTimelinePayloadValue(event, 'argumentsChunk')
  if (typeof raw !== 'string') return ''
  return raw
}

function getTimelineToolResult(event: TimelineEvent) {
  const error = getTimelinePayloadValue(event, 'error')
  if (typeof error === 'string' && error.trim()) {
    return JSON.stringify({ error }, null, 2)
  }

  const result = getTimelinePayloadValue(event, 'result')
  if (typeof result === 'string') return result
  if (result === null || result === undefined) return ''
  try {
    return JSON.stringify(result, null, 2)
  } catch {
    return String(result)
  }
}

function getTimelineToolResultStatus(event: TimelineEvent) {
  const error = getTimelinePayloadValue(event, 'error')
  return typeof error === 'string' && error.trim() ? 'error' : 'done'
}

function isTimelineToolEvent(event: TimelineEvent) {
  return event.event_type === 'assistant_tool_call' || event.event_type === 'assistant_tool_result'
}

function getTimelineToolDisplayText(event: TimelineEvent) {
  if (event.event_type === 'assistant_tool_call') {
    return getTimelineToolArguments(event)
  }
  if (event.event_type === 'assistant_tool_result') {
    return getTimelineToolResult(event)
  }
  return ''
}

function formatToolStepStatus(status: ToolStepStatus) {
  if (status === 'running') return '执行中'
  if (status === 'done') return '已完成'
  return '执行失败'
}

function getToolExecutionGroup(turnId: string, firstEventId: string) {
  const groups = turnToolExecutionGroupsByTurnId.value[turnId] || []
  for (const group of groups) {
    if (group.firstEventId === firstEventId) return group
  }
  return null
}

function getToolExecutionGroupKey(turnId: string, firstEventId: string) {
  const group = getToolExecutionGroup(turnId, firstEventId)
  return group?.key || `${turnId}:${firstEventId}`
}

function isToolExecutionGroupCollapsed(turnId: string, firstEventId: string) {
  const key = getToolExecutionGroupKey(turnId, firstEventId)
  return compactToolExecutionCollapsedByGroupKey.value[key] ?? true
}

function toggleToolExecutionGroup(turnId: string, firstEventId: string) {
  const key = getToolExecutionGroupKey(turnId, firstEventId)
  compactToolExecutionCollapsedByGroupKey.value[key] = !isToolExecutionGroupCollapsed(turnId, firstEventId)
  compactToolExecutionVersion.value += 1
}

function getToolExecutionGroupSteps(turnId: string, firstEventId: string) {
  const group = getToolExecutionGroup(turnId, firstEventId)
  return group?.steps || []
}

function shouldRenderCompactToolSummary(turnId: string, firstEventId: string) {
  if (isToolDisplayFull.value) return false
  const group = getToolExecutionGroup(turnId, firstEventId)
  return Boolean(group && group.steps.length > 0)
}

function toggleTaskMonitor() {
  taskMonitorCollapsed.value = !taskMonitorCollapsed.value
}

function toggleMonitorSection(section: 'todos' | 'artifacts' | 'skills') {
  monitorSectionsOpen.value[section] = !monitorSectionsOpen.value[section]
}

function mapToolNameToMonitorSkill(toolName: string) {
  if (!toolName) return ''
  if (toolName.startsWith('mcp__')) {
    return renderToolLabel(toolName)
  }
  if (toolName === 'browser' || toolName === 'browser_navigate' || toolName === 'web_fetch' || toolName === 'web_search') {
    return 'agent-browser'
  }
  if (toolName === 'desktop') {
    return 'desktop-automation'
  }
  if (toolName === 'skills_install_from_repo') {
    return 'skill-installer'
  }
  return ''
}

function isArtifactToolName(toolName: string) {
  return toolName === 'workspace_write_file' || toolName === 'workspace_edit_file' || toolName === 'desktop'
}

function resolveArtifactAction(toolName: string) {
  if (toolName === 'workspace_edit_file') return '修改'
  if (toolName === 'workspace_write_file') return '生成'
  if (toolName === 'desktop') return '输出'
  return '产物'
}

function extractArtifactPathFromCall(toolName: string, args: Record<string, unknown> | null) {
  if (!args) return ''
  if (toolName === 'workspace_write_file' || toolName === 'workspace_edit_file') {
    return readNestedString(args, 'path')
  }
  if (toolName === 'desktop') {
    return readNestedString(args, 'params.path')
  }
  return ''
}

function extractArtifactPathFromResult(result: Record<string, unknown> | null) {
  if (!result) return ''
  return (
    readNestedString(result, 'path') ||
    readNestedString(result, 'file_path') ||
    readNestedString(result, 'data.path') ||
    readNestedString(result, 'result.path')
  )
}

function buildCompactToolExecutionGroups(turnId: string, turnEvents: TimelineEvent[]) {
  const groups: CompactToolExecutionGroup[] = []
  let currentGroupEvents: TimelineEvent[] = []

  const flushGroup = () => {
    if (currentGroupEvents.length === 0) return
    const firstEventId = currentGroupEvents[0].id
    const steps = buildTurnToolExecutionSteps(currentGroupEvents)
    if (steps.length > 0) {
      groups.push({
        key: `${turnId}:${firstEventId}`,
        firstEventId,
        steps
      })
    }
    currentGroupEvents = []
  }

  for (const event of turnEvents) {
    if (isTimelineToolEvent(event)) {
      currentGroupEvents.push(event)
      continue
    }
    flushGroup()
  }

  flushGroup()
  return groups
}

function buildTurnToolExecutionSteps(turnEvents: TimelineEvent[]): CompactToolExecutionStep[] {
  const steps: CompactToolExecutionStep[] = []
  const callIndexByToolCallId = new Map<string, number>()
  const runningIndexes: number[] = []

  for (const event of turnEvents) {
    if (!isTimelineToolEvent(event)) continue

    if (event.event_type === 'assistant_tool_call') {
      const rawName = String(getTimelinePayloadValue(event, 'name') || '')
      const normalizedToolName = normalizeToolName(rawName)
      const args = parseJsonObjectLoose(getTimelinePayloadValue(event, 'argumentsChunk'))
      const detail = getTimelineToolCompactDetail(event, turnEvents)
      const artifactPath = extractArtifactPathFromCall(normalizedToolName, args)

      steps.push({
        id: event.tool_call_id || event.id,
        toolName: normalizedToolName,
        title: getTimelineToolName(event),
        detail,
        status: 'running',
        artifactPath
      })

      const createdIndex = steps.length - 1
      runningIndexes.push(createdIndex)
      if (event.tool_call_id) {
        callIndexByToolCallId.set(event.tool_call_id, createdIndex)
      }
      continue
    }

    const resultStatus: ToolStepStatus = getTimelineToolResultStatus(event) === 'error' ? 'error' : 'done'
    const resultObj = parseJsonObjectLoose(getTimelinePayloadValue(event, 'result'))
    const resultDetail = getTimelineToolCompactDetail(event, turnEvents)
    const resultArtifactPath = extractArtifactPathFromResult(resultObj)

    let targetIndex = -1
    if (event.tool_call_id && callIndexByToolCallId.has(event.tool_call_id)) {
      targetIndex = callIndexByToolCallId.get(event.tool_call_id) ?? -1
    } else {
      for (let i = runningIndexes.length - 1; i >= 0; i -= 1) {
        const index = runningIndexes[i]
        if (steps[index] && steps[index].status === 'running') {
          targetIndex = index
          runningIndexes.splice(i, 1)
          break
        }
      }
    }

    if (targetIndex >= 0 && steps[targetIndex]) {
      const step = steps[targetIndex]
      step.status = resultStatus
      step.detail = resultDetail || step.detail
      if (resultArtifactPath) {
        step.artifactPath = resultArtifactPath
      }
      const runningPosition = runningIndexes.lastIndexOf(targetIndex)
      if (runningPosition >= 0) {
        runningIndexes.splice(runningPosition, 1)
      }
      continue
    }

    const rawName = String(getTimelinePayloadValue(event, 'name') || '')
    const normalizedToolName = normalizeToolName(rawName)
    steps.push({
      id: event.tool_call_id || event.id,
      toolName: normalizedToolName,
      title: getTimelineToolName(event),
      detail: resultDetail,
      status: resultStatus,
      artifactPath: resultArtifactPath
    })
  }

  return steps
}

function getTimelineToolCompactDetail(event: TimelineEvent, turnEvents: TimelineEvent[]) {
  const cacheKey = `${event.id}:${event.seq}`
  const cached = timelineToolCompactDetailCache.get(cacheKey)
  if (cached !== undefined) return cached

  const toolName = String(getTimelinePayloadValue(event, 'name') || '')
  const normalized = normalizeToolName(toolName)
  let detail = ''

  if (event.event_type === 'assistant_tool_call') {
    const callArgs = parseJsonObjectLoose(getTimelinePayloadValue(event, 'argumentsChunk'))
    detail = summarizeToolAction(normalized, callArgs)
    setBoundedCacheValue(timelineToolCompactDetailCache, cacheKey, detail, TOOL_DETAIL_CACHE_MAX_ENTRIES)
    return detail
  }

  if (event.event_type === 'assistant_tool_result') {
    const error = getTimelinePayloadValue(event, 'error')
    if (typeof error === 'string' && error.trim()) {
      detail = `错误: ${shortenText(error, 72)}`
      setBoundedCacheValue(timelineToolCompactDetailCache, cacheKey, detail, TOOL_DETAIL_CACHE_MAX_ENTRIES)
      return detail
    }

    const linkedCallSummary = findLinkedToolCallSummary(event, turnEvents)
    if (linkedCallSummary) {
      detail = linkedCallSummary
      setBoundedCacheValue(timelineToolCompactDetailCache, cacheKey, detail, TOOL_DETAIL_CACHE_MAX_ENTRIES)
      return detail
    }

    const resultObject = parseJsonObjectLoose(getTimelinePayloadValue(event, 'result'))
    detail = summarizeToolResult(normalized, resultObject)
    setBoundedCacheValue(timelineToolCompactDetailCache, cacheKey, detail, TOOL_DETAIL_CACHE_MAX_ENTRIES)
    return detail
  }

  setBoundedCacheValue(timelineToolCompactDetailCache, cacheKey, detail, TOOL_DETAIL_CACHE_MAX_ENTRIES)
  return detail
}

function findLinkedToolCallSummary(event: TimelineEvent, turnEvents: TimelineEvent[]) {
  const targetToolCallId = event.tool_call_id || null
  if (!targetToolCallId) return ''

  for (let i = turnEvents.length - 1; i >= 0; i -= 1) {
    const candidate = turnEvents[i]
    if (candidate.id === event.id) continue
    if (candidate.event_type !== 'assistant_tool_call') continue
    if (candidate.tool_call_id !== targetToolCallId) continue
    const callArgs = parseJsonObjectLoose(getTimelinePayloadValue(candidate, 'argumentsChunk'))
    const toolName = String(getTimelinePayloadValue(candidate, 'name') || '')
    const summary = summarizeToolAction(normalizeToolName(toolName), callArgs)
    if (summary) return summary
  }
  return ''
}

function summarizeToolAction(toolName: string, args: Record<string, unknown> | null) {
  if (!args) return ''

  const pick = (...keys: string[]) => {
    for (const key of keys) {
      const value = readNestedString(args, key)
      if (value) return value
    }
    return ''
  }

  if (toolName === 'browser') {
    const action = pick('action')
    const url = pick('params.url', 'url')
    const selector = pick('params.selector', 'selector')
    if (action === 'navigate' && url) return `打开链接: ${shortenUrl(url)}`
    if (action === 'click' && selector) return `点击元素: ${shortenText(selector, 52)}`
    if (action === 'type') {
      const target = selector || pick('params.text', 'text')
      if (target) return `输入: ${shortenText(target, 52)}`
    }
    if (action) return `浏览器动作: ${action}`
  }

  if (toolName === 'desktop') {
    const action = pick('action')
    const windowId = pick('params.window_id', 'params.id', 'params.hwnd')
    const controlId = pick('params.control_id', 'params.id')
    const text = pick('params.text')
    const path = pick('params.path')
    if (action === 'select_window' && windowId) return `选择窗口: ${windowId}`
    if (action === 'launch_application') {
      const command = pick('params.command')
      if (command) return `启动程序: ${shortenText(command, 56)}`
    }
    if (action === 'click_input' && controlId) return `点击控件: ${controlId}`
    if (action === 'set_edit_text') {
      if (text) return `输入文本: ${shortenText(text, 40)}`
      if (controlId) return `设置控件文本: ${controlId}`
    }
    if (action === 'keyboard_input') {
      const keys = pick('params.keys')
      if (keys) return `键盘输入: ${shortenText(keys, 52)}`
    }
    if (action === 'capture_window_screenshot' || action === 'capture_desktop_screenshot') {
      return '截取桌面截图'
    }
    if (action && path) return `${action}: ${shortenPath(path)}`
    if (action) return `桌面动作: ${action}`
  }

  if (toolName === 'browser_navigate') {
    const url = pick('url')
    if (url) return `打开链接: ${shortenUrl(url)}`
  }

  if (toolName === 'web_fetch') {
    const url = pick('url')
    if (url) return `抓取网页: ${shortenUrl(url)}`
  }

  if (toolName === 'web_search') {
    const query = pick('query', 'q')
    if (query) return `搜索: ${shortenText(query, 56)}`
  }

  if (toolName === 'workspace_run_command') {
    const command = pick('command')
    if (command) return `执行命令: ${shortenText(command, 68)}`
  }

  if (toolName === 'workspace_list_directory') {
    const path = pick('path')
    if (path) return `查看目录: ${shortenPath(path)}`
    return '查看目录内容'
  }

  if (toolName === 'workspace_read_file') {
    const path = pick('path')
    if (path) return `读取文件: ${shortenPath(path)}`
  }

  if (toolName === 'workspace_write_file' || toolName === 'workspace_edit_file') {
    const path = pick('path')
    if (path) return `写入文件: ${shortenPath(path)}`
  }

  if (toolName === 'workspace_grep') {
    const pattern = pick('pattern')
    const path = pick('path')
    if (pattern && path) return `搜索 "${shortenText(pattern, 28)}" 于 ${shortenPath(path)}`
    if (pattern) return `搜索内容: ${shortenText(pattern, 56)}`
  }

  if (toolName === 'workspace_glob') {
    const pattern = pick('pattern')
    const path = pick('path')
    if (pattern && path) return `匹配 ${shortenText(pattern, 32)} 于 ${shortenPath(path)}`
    if (pattern) return `匹配文件: ${shortenText(pattern, 56)}`
  }

  if (toolName === 'skills_install_from_repo') {
    const repo = pick('repo_url', 'repoUrl')
    if (repo) return `安装技能: ${shortenUrl(repo)}`
  }

  const fallback = firstObjectEntrySummary(args)
  return fallback ? `参数: ${fallback}` : ''
}

function summarizeToolResult(toolName: string, result: Record<string, unknown> | null) {
  if (!result) return ''

  const pick = (...keys: string[]) => {
    for (const key of keys) {
      const value = readNestedString(result, key)
      if (value) return value
    }
    return ''
  }

  if (toolName === 'browser' || toolName === 'browser_navigate') {
    const url = pick('url', 'current_url', 'final_url')
    if (url) return `页面: ${shortenUrl(url)}`
  }

  if (toolName === 'web_fetch') {
    const url = pick('url')
    const status = pick('status', 'status_code')
    if (url && status) return `抓取完成: ${shortenUrl(url)} (${status})`
    if (url) return `抓取完成: ${shortenUrl(url)}`
  }

  if (toolName === 'web_search') {
    const query = pick('query', 'q')
    if (query) return `搜索完成: ${shortenText(query, 56)}`
  }

  if (toolName === 'desktop') {
    const ok = pick('ok')
    const path = pick('data.path')
    if (ok === 'true' && path) return `截图: ${shortenPath(path)}`
    const selectedWindow = pick('data.selected_window.title')
    if (selectedWindow) return `窗口: ${shortenText(selectedWindow, 56)}`
    const error = pick('error')
    if (error) return `失败: ${shortenText(error, 56)}`
  }

  const fallback = firstObjectEntrySummary(result)
  return fallback ? `结果: ${fallback}` : ''
}

function parseJsonObjectLoose(value: unknown): Record<string, unknown> | null {
  if (!value) return null
  if (typeof value === 'object' && !Array.isArray(value)) {
    return value as Record<string, unknown>
  }
  if (typeof value !== 'string') return null

  let candidate = value.trim()
  if (!candidate) return null

  for (let i = 0; i < 3; i += 1) {
    try {
      const parsed = JSON.parse(candidate)
      if (parsed && typeof parsed === 'object' && !Array.isArray(parsed)) {
        return parsed as Record<string, unknown>
      }
      if (typeof parsed === 'string') {
        const next = parsed.trim()
        if (!next || next === candidate) break
        candidate = next
        continue
      }
      break
    } catch {
      break
    }
  }
  return null
}

function readNestedString(source: Record<string, unknown>, path: string) {
  const parts = path.split('.')
  let current: unknown = source
  for (const part of parts) {
    if (!current || typeof current !== 'object' || Array.isArray(current)) return ''
    current = (current as Record<string, unknown>)[part]
  }
  if (typeof current === 'string') {
    const trimmed = current.trim()
    return trimmed || ''
  }
  if (typeof current === 'number' || typeof current === 'boolean') {
    return String(current)
  }
  return ''
}

function firstObjectEntrySummary(source: Record<string, unknown>) {
  const entries = Object.entries(source)
  for (const [key, raw] of entries) {
    if (raw === null || raw === undefined) continue
    if (typeof raw === 'object') continue
    const text = String(raw).trim()
    if (!text) continue
    return `${key}=${shortenText(text, 48)}`
  }
  return ''
}

function shortenText(value: string, maxLength: number) {
  const normalized = value.replace(/\s+/g, ' ').trim()
  if (normalized.length <= maxLength) return normalized
  return `${normalized.slice(0, maxLength - 1)}...`
}

function shortenUrl(url: string) {
  return truncateMiddle(url, 72)
}

function shortenPath(path: string) {
  return truncateMiddle(path, 68)
}

function isTimelineReasoningCollapsed(eventId: string) {
  return timelineReasoningCollapsedByEventId.value[eventId] ?? false
}

function toggleTimelineReasoning(eventId: string) {
  timelineReasoningCollapsedByEventId.value[eventId] = !isTimelineReasoningCollapsed(eventId)
  timelineReasoningCollapsedVersion.value += 1
}

function setBoundedCacheValue<T>(cache: Map<string, T>, key: string, value: T, maxEntries: number) {
  cache.set(key, value)
  if (cache.size <= maxEntries) return
  const firstKey = cache.keys().next()
  if (!firstKey.done) {
    cache.delete(firstKey.value)
  }
}

function getComposerText() {
  return composerInputRef.value?.value || ''
}

function setComposerText(value: string) {
  const input = composerInputRef.value
  if (!input) return
  input.value = value
}

function handleComposerEnter(event: KeyboardEvent) {
  if (event.isComposing || event.keyCode === 229) return
  if (isCurrentConversationStreaming.value) return
  void sendMessage()
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
  if (toolName === 'workspace_run_command') {
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
  if (toolName === 'workspace_run_command' && typeof args.command === 'string') {
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

function escapeHtml(value: string) {
  return value
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;')
    .replace(/'/g, '&#39;')
}

function isSvgCodeBlock(code: string, infostring?: string) {
  const lang = (infostring || '').trim().toLowerCase()
  if (lang === 'svg' || lang === 'image/svg+xml') return true
  const trimmed = code.trim()
  return /^<svg[\s>]/i.test(trimmed) && /<\/svg>\s*$/i.test(trimmed)
}

const markdownRenderer = new marked.Renderer()
markdownRenderer.code = (code: string, infostring: string | undefined, escaped: boolean) => {
  const language = (infostring || '').trim()
  const languageClass = language ? ` class="language-${escapeHtml(language)}"` : ''
  const safeCode = escaped ? code : escapeHtml(code)
  const codeBlockHtml = `
<div class="md-code-block">
  <button class="md-code-copy-btn" type="button" data-action="copy-code">复制</button>
  <pre><code${languageClass}>${safeCode}</code></pre>
</div>`
  if (!isSvgCodeBlock(code, infostring)) {
    return codeBlockHtml
  }

  const svgSource = code.trim()
  const previewSrc = `data:image/svg+xml;utf8,${encodeURIComponent(svgSource)}`
  return `
<div class="svg-preview-block">
  ${codeBlockHtml}
  <div class="svg-preview-label">SVG 预览</div>
  <div class="svg-preview-canvas">
    <img class="svg-preview-image" src="${previewSrc}" alt="SVG 预览" loading="lazy" />
  </div>
</div>`
}

function renderMarkdown(content: string) {
  const source = content || ''
  const cached = markdownCache.get(source)
  if (cached !== undefined) {
    return cached
  }
  const rendered = marked.parse(source, { async: false, renderer: markdownRenderer }) as string
  setBoundedCacheValue(markdownCache, source, rendered, MARKDOWN_CACHE_MAX_ENTRIES)
  return rendered
}

function isExternalHttpUrl(value: string) {
  return /^https?:\/\//i.test(value)
}

async function copyTextToClipboard(text: string) {
  if (!text) return false
  try {
    await navigator.clipboard.writeText(text)
    return true
  } catch {
    const textarea = document.createElement('textarea')
    textarea.value = text
    textarea.style.position = 'fixed'
    textarea.style.opacity = '0'
    textarea.style.pointerEvents = 'none'
    document.body.appendChild(textarea)
    textarea.focus()
    textarea.select()
    let copied = false
    try {
      copied = document.execCommand('copy')
    } catch {
      copied = false
    }
    document.body.removeChild(textarea)
    return copied
  }
}

async function handleMarkdownLinkClick(event: MouseEvent) {
  const target = event.target
  if (!(target instanceof HTMLElement)) return

  const copyTrigger = target.closest('[data-action="copy-code"]') as HTMLButtonElement | null
  if (copyTrigger) {
    event.preventDefault()
    event.stopPropagation()
    const block = copyTrigger.closest('.md-code-block') as HTMLElement | null
    const codeElement = block?.querySelector('code') as HTMLElement | null
    const content = codeElement?.textContent || ''
    if (!content.trim()) {
      ElMessage.warning('没有可复制的代码')
      return
    }
    const copied = await copyTextToClipboard(content)
    if (!copied) {
      ElMessage.error('复制失败')
      return
    }
    copyTrigger.textContent = '已复制'
    copyTrigger.classList.add('copied')
    window.setTimeout(() => {
      copyTrigger.textContent = '复制'
      copyTrigger.classList.remove('copied')
    }, 1200)
    return
  }

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

function uploadIcon(extension: string) {
  if (extension === 'pdf') return 'picture_as_pdf'
  if (['png', 'jpg', 'jpeg', 'gif', 'bmp', 'webp'].includes(extension)) return 'image'
  if (['doc', 'docx'].includes(extension)) return 'description'
  if (['ppt', 'pptx'].includes(extension)) return 'slideshow'
  if (['xls', 'xlsx', 'xlsm'].includes(extension)) return 'table_chart'
  return 'insert_drive_file'
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
