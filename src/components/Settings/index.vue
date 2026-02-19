<template>
  <el-dialog
    v-model="dialogVisible"
    title="Settings"
    width="600px"
    :close-on-click-modal="false"
  >
    <el-tabs v-model="activeTab">
      <!-- Account / Model status -->
      <el-tab-pane label="模型与账户" name="account">
        <el-alert
          title="AI 请求由 Petool 中转服务统一处理"
          type="success"
          :closable="false"
          description="所有 AI 对话均通过您的账户 Token 走中转后端，无需填写 API Key。请前往「账户」页面查看余额和用量。"
          style="margin-bottom: 20px"
        />
        <el-descriptions :column="1" border size="small">
          <el-descriptions-item label="登录状态">
            {{ configStore.config.petool_token ? '✅ 已登录' : '❌ 未登录，请前往账户页登录' }}
          </el-descriptions-item>
          <el-descriptions-item label="文本模型">{{ configStore.config.model || '未设置' }}</el-descriptions-item>
          <el-descriptions-item label="中转服务地址">{{ configStore.config.petool_api_base || 'http://localhost:8000' }}</el-descriptions-item>
        </el-descriptions>
      </el-tab-pane>

      <!-- Appearance -->
      <el-tab-pane label="Appearance" name="appearance">
        <el-form label-width="120px">
          <el-form-item label="Tool Display">
            <el-radio-group v-model="localConfig.tool_display_mode">
              <el-radio value="compact">Compact (Recommended)</el-radio>
              <el-radio value="full">Full</el-radio>
            </el-radio-group>
          </el-form-item>

          <el-form-item label="Auto Approvals">
            <el-switch v-model="localConfig.auto_approve_tool_requests" />
            <div class="setting-hint">
              When enabled, tool calls run without asking every time (explicit deny rules still apply).
            </div>
          </el-form-item>
        </el-form>
      </el-tab-pane>

      <!-- Browser -->
      <el-tab-pane label="Browser" name="browser">
        <el-form :model="localConfig.browser" label-width="160px">
          <el-form-item label="Enable Browser Tool">
            <el-switch v-model="localConfig.browser.enabled" />
          </el-form-item>

          <el-form-item label="Default Profile">
            <el-select v-model="localConfig.browser.default_profile" style="width: 100%">
              <el-option
                v-for="name in browserProfileNames"
                :key="name"
                :label="name"
                :value="name"
              />
            </el-select>
          </el-form-item>

          <el-form-item label="Evaluate Enabled">
            <el-switch v-model="localConfig.browser.evaluate_enabled" />
          </el-form-item>

          <el-form-item label="Allow Private Network">
            <el-switch v-model="localConfig.browser.allow_private_network" />
          </el-form-item>

          <el-form-item label="Performance Preset">
            <el-select v-model="localConfig.browser.performance_preset" style="width: 220px">
              <el-option label="Safe" value="safe" />
              <el-option label="Balanced" value="balanced" />
              <el-option label="Fast" value="fast" />
            </el-select>
          </el-form-item>

          <el-form-item label="Capture Response Bodies">
            <el-switch v-model="localConfig.browser.capture_response_bodies" />
          </el-form-item>

          <el-form-item label="Default Act Timeout (ms)">
            <el-input-number
              v-model="localConfig.browser.default_act_timeout_ms"
              :min="250"
              :max="20000"
              :step="50"
              style="width: 220px"
            />
          </el-form-item>

          <el-form-item label="Timeout (ms)">
            <el-input-number
              v-model="localConfig.browser.operation_timeout_ms"
              :min="1000"
              :max="120000"
              :step="1000"
              style="width: 220px"
            />
          </el-form-item>

          <el-divider content-position="left">Profile: {{ activeBrowserProfileName }}</el-divider>

          <el-alert
            title="连接模式说明"
            type="info"
            :closable="false"
            description="默认通过外部 Chrome 接管：使用 Executable Path 启动，附带 --remote-debugging-port、--user-data-dir、--window-size。CDP URL 用于接管你手动启动的调试浏览器。"
            style="margin-bottom: 12px"
          />

          <el-form-item label="Headless">
            <el-switch v-model="activeBrowserProfile.headless" />
          </el-form-item>

          <el-form-item label="CDP URL">
            <el-input
              v-model="activeBrowserProfile.cdp_url"
              placeholder="例如: http://127.0.0.1:9222"
            />
          </el-form-item>

          <el-form-item label="Executable Path">
            <el-input
              v-model="activeBrowserProfile.executable_path"
              placeholder="例如: C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe"
            />
          </el-form-item>

          <el-form-item label="User Data Dir">
            <el-input
              v-model="activeBrowserProfile.user_data_dir"
              placeholder="可选；留空则使用 PETool 默认目录"
            />
          </el-form-item>

          <el-form-item label="Viewport">
            <div class="viewport-row">
              <el-input-number
                v-model="activeBrowserProfile.viewport.width"
                :min="320"
                :max="8192"
              />
              <span class="viewport-sep">x</span>
              <el-input-number
                v-model="activeBrowserProfile.viewport.height"
                :min="240"
                :max="8192"
              />
            </div>
          </el-form-item>

          <el-form-item label="Profile Data">
            <div class="browser-actions">
              <el-button @click="openBrowserProfileDir">
                <el-icon><FolderOpened /></el-icon>
                Open Profile Directory
              </el-button>
              <el-button type="danger" plain @click="resetBrowserProfile">
                <el-icon><RefreshRight /></el-icon>
                Reset Profile
              </el-button>
            </div>
          </el-form-item>
        </el-form>
      </el-tab-pane>

      <!-- Desktop -->
      <el-tab-pane label="Desktop" name="desktop">
        <el-form :model="localConfig.desktop" label-width="180px">
          <el-form-item label="Enable Desktop Tool">
            <el-switch v-model="localConfig.desktop.enabled" />
          </el-form-item>

          <el-form-item label="Timeout (ms)">
            <el-input-number
              v-model="localConfig.desktop.operation_timeout_ms"
              :min="1000"
              :max="120000"
              :step="1000"
              style="width: 220px"
            />
          </el-form-item>

          <el-form-item label="Control Cache TTL (ms)">
            <el-input-number
              v-model="localConfig.desktop.control_cache_ttl_ms"
              :min="250"
              :max="600000"
              :step="250"
              style="width: 220px"
            />
          </el-form-item>

          <el-form-item label="Max Controls">
            <el-input-number
              v-model="localConfig.desktop.max_controls"
              :min="10"
              :max="10000"
              :step="10"
              style="width: 220px"
            />
          </el-form-item>

          <el-form-item label="Screenshot Keep Count">
            <el-input-number
              v-model="localConfig.desktop.screenshot_keep_count"
              :min="20"
              :max="10000"
              :step="10"
              style="width: 220px"
            />
          </el-form-item>

          <el-form-item label="Approval Mode">
            <el-select v-model="localConfig.desktop.approval_mode" style="width: 260px">
              <el-option label="High-Risk Only (Recommended)" value="high_risk_only" />
              <el-option label="Always Ask" value="always_ask" />
              <el-option label="Always Allow" value="always_allow" />
            </el-select>
          </el-form-item>

          <el-form-item label="Screenshot Directory">
            <el-input
              v-model="localConfig.desktop.screenshot_dir"
              placeholder="Optional. Empty uses app log directory desktop-shots."
            />
          </el-form-item>
        </el-form>
      </el-tab-pane>

      <!-- Automation -->
      <el-tab-pane label="Automation" name="automation">
        <AutomationPanel v-model:automation="localConfig.automation" />
      </el-tab-pane>

      <!-- MCP Servers -->
      <el-tab-pane label="MCP Servers" name="mcp">
        <div class="mcp-list">
          <div
            v-for="(server, index) in localConfig.mcp_servers"
            :key="index"
            class="mcp-item"
          >
            <div class="mcp-info">
              <strong>{{ server.name }}</strong>
              <span class="mcp-type">{{ server.transport.type }}</span>
            </div>
            <div class="mcp-actions">
              <el-switch v-model="server.enabled" size="small" />
              <el-button
                type="danger"
                size="small"
                text
                @click="removeMcpServer(index)"
              >
                <el-icon><Delete /></el-icon>
              </el-button>
            </div>
          </div>

          <el-empty
            v-if="localConfig.mcp_servers.length === 0"
            description="No MCP servers configured"
            :image-size="60"
          />

          <el-button @click="showAddMcpDialog = true" style="width: 100%; margin-top: 12px">
            <el-icon><Plus /></el-icon>
            Add MCP Server
          </el-button>
        </div>
      </el-tab-pane>
    </el-tabs>

    <template #footer>
      <el-button @click="dialogVisible = false">Cancel</el-button>
      <el-button type="primary" @click="handleSave" :loading="saving">
        Save
      </el-button>
    </template>
  </el-dialog>

  <!-- Add MCP Server Dialog -->
  <el-dialog
    v-model="showAddMcpDialog"
    title="Add MCP Server"
    width="500px"
  >
    <el-form :model="newMcpServer" label-width="100px">
      <el-form-item label="Name">
        <el-input v-model="newMcpServer.name" placeholder="Server name" />
      </el-form-item>

      <el-form-item label="Transport">
        <el-radio-group v-model="newMcpServer.transportType">
          <el-radio value="stdio">Stdio</el-radio>
          <el-radio value="http">HTTP</el-radio>
        </el-radio-group>
      </el-form-item>

      <template v-if="newMcpServer.transportType === 'stdio'">
        <el-form-item label="Command">
          <el-input v-model="newMcpServer.command" placeholder="python" />
        </el-form-item>
        <el-form-item label="Arguments">
          <el-input
            v-model="newMcpServer.args"
            placeholder="-m mcp_server"
          />
        </el-form-item>
      </template>

      <template v-else>
        <el-form-item label="URL">
          <el-input v-model="newMcpServer.url" placeholder="http://localhost:3000" />
        </el-form-item>
      </template>
    </el-form>

    <template #footer>
      <el-button @click="showAddMcpDialog = false">Cancel</el-button>
      <el-button type="primary" @click="addMcpServer">Add</el-button>
    </template>
  </el-dialog>
</template>

<script setup lang="ts">
import { ref, watch, computed } from 'vue'
import {
  useConfigStore,
  type BrowserConfig,
  type DesktopConfig,
  type AutomationConfig,
  type BrowserProfileConfig,
  type Config,
  type McpServerConfig
} from '@/stores/config'
import { ElMessage, ElMessageBox } from 'element-plus'
import { FolderOpened, Plus, Delete, RefreshRight } from '@element-plus/icons-vue'
import { invoke } from '@tauri-apps/api/core'
import AutomationPanel from './AutomationPanel.vue'

interface Props {
  modelValue?: boolean
}

interface Emits {
  (e: 'update:modelValue', value: boolean): void
}

const props = defineProps<Props>()
const emit = defineEmits<Emits>()

const configStore = useConfigStore()
const activeTab = ref('api')
const saving = ref(false)
const showAddMcpDialog = ref(false)

function deepClone<T>(value: T): T {
  return JSON.parse(JSON.stringify(value)) as T
}

const defaultBrowserConfig: BrowserConfig = {
  enabled: true,
  default_profile: 'openclaw',
  evaluate_enabled: false,
  allow_private_network: false,
  performance_preset: 'balanced',
  capture_response_bodies: false,
  default_act_timeout_ms: 1400,
  operation_timeout_ms: 20000,
  profiles: {
    openclaw: {
      engine: 'chrome',
      headless: false,
      executable_path: null,
      cdp_url: null,
      user_data_dir: null,
      color: '#FF6A00',
      viewport: {
        width: 1280,
        height: 800
      }
    }
  }
}

const defaultDesktopConfig: DesktopConfig = {
  enabled: true,
  operation_timeout_ms: 20000,
  control_cache_ttl_ms: 120000,
  max_controls: 800,
  screenshot_dir: null,
  screenshot_keep_count: 200,
  approval_mode: 'high_risk_only'
}

const defaultAutomationConfig: AutomationConfig = {
  enabled: true,
  max_concurrent_runs: 1,
  close_behavior: 'ask',
  heartbeat: {
    enabled: true,
    every_minutes: 30,
    target_conversation_id: null,
    prompt: 'Read HEARTBEAT.md if it exists in workspace and check pending tasks. If nothing needs attention, reply HEARTBEAT_OK.',
    model: null,
    workspace_directory: null,
    tool_whitelist: [
      'workspace_list_directory',
      'workspace_read_file',
      'workspace_glob',
      'workspace_grep',
      'workspace_codesearch',
      'workspace_lsp_symbols',
      'web_fetch',
      'web_search',
      'sessions_list',
      'sessions_history',
      'sessions_send',
      'sessions_spawn',
      'workspace_write_file',
      'workspace_edit_file',
      'workspace_apply_patch'
    ]
  }
}

const defaultConfig: Config = {
  api_key: '',
  api_base: 'https://open.bigmodel.cn/api/paas/v4',
  clawhub_api_key: '',
  clawhub_api_base: 'https://clawhub.ai',
  ark_api_key: '',
  ark_api_base: 'https://ark.cn-beijing.volces.com/api/v3',
  minimax_api_key: '',
  image_model: 'doubao-seedream-4-5-251128',
  image_understand_model: 'glm-4.6v',
  video_model: 'doubao-seedance-1-0-pro-250528',
  image_size: '2K',
  image_watermark: true,
  model: 'glm-5',
  system_prompt: '',
  work_directory: '',
  conversation_workspaces: {},
  theme: 'light',
  tool_display_mode: 'compact',
  mcp_servers: [],
  tool_permissions: {},
  tool_path_permissions: [],
  auto_approve_tool_requests: false,
  autostart_enabled: false,
  downloads_directory: null,
  notifications: {
    sound_enabled: false,
    break_reminder_enabled: true,
    task_completed_enabled: true
  },
  browser: deepClone(defaultBrowserConfig),
  desktop: deepClone(defaultDesktopConfig),
  automation: deepClone(defaultAutomationConfig)
}

const localConfig = ref<Config>({
  ...defaultConfig
})

interface NewMcpServerForm {
  name: string
  transportType: 'stdio' | 'http'
  command: string
  args: string
  url: string
}

function createDefaultMcpServerForm(): NewMcpServerForm {
  return {
    name: '',
    transportType: 'stdio',
    command: '',
    args: '',
    url: ''
  }
}

const newMcpServer = ref<NewMcpServerForm>(createDefaultMcpServerForm())

const dialogVisible = computed({
  get: () => props.modelValue ?? false,
  set: (val) => emit('update:modelValue', val)
})

function ensureBrowserConfig(target: Config) {
  if (!target.browser) {
    target.browser = deepClone(defaultBrowserConfig)
  }
  if (!target.browser.performance_preset) {
    target.browser.performance_preset = 'balanced'
  }
  if (!['safe', 'balanced', 'fast'].includes(target.browser.performance_preset)) {
    target.browser.performance_preset = 'balanced'
  }
  if (target.browser.capture_response_bodies === undefined) {
    target.browser.capture_response_bodies = false
  }
  if (
    target.browser.default_act_timeout_ms === undefined ||
    Number.isNaN(Number(target.browser.default_act_timeout_ms))
  ) {
    target.browser.default_act_timeout_ms = 1400
  }
  target.browser.default_act_timeout_ms = Math.max(
    250,
    Math.min(20000, Math.trunc(target.browser.default_act_timeout_ms))
  )
  if (!target.browser.profiles || Object.keys(target.browser.profiles).length === 0) {
    target.browser.profiles = deepClone(defaultBrowserConfig.profiles)
  }
  if (!target.browser.default_profile || !target.browser.profiles[target.browser.default_profile]) {
    target.browser.default_profile = Object.keys(target.browser.profiles)[0] || 'openclaw'
  }
  const profile = target.browser.profiles[target.browser.default_profile]
  if (!profile.engine) {
    profile.engine = 'chrome'
  }
  if (profile.executable_path === undefined) {
    profile.executable_path = null
  }
  if (profile.cdp_url === undefined) {
    profile.cdp_url = null
  }
  if (profile.user_data_dir === undefined) {
    profile.user_data_dir = null
  }
  if (!profile.viewport) {
    profile.viewport = { width: 1280, height: 800 }
  }
}

function ensureDesktopConfig(target: Config) {
  if (!target.desktop) {
    target.desktop = deepClone(defaultDesktopConfig)
  }
  if (target.desktop.enabled === undefined) {
    target.desktop.enabled = defaultDesktopConfig.enabled
  }
  if (
    target.desktop.operation_timeout_ms === undefined ||
    Number.isNaN(Number(target.desktop.operation_timeout_ms))
  ) {
    target.desktop.operation_timeout_ms = defaultDesktopConfig.operation_timeout_ms
  }
  target.desktop.operation_timeout_ms = Math.max(
    1000,
    Math.min(120000, Math.trunc(target.desktop.operation_timeout_ms))
  )
  if (
    target.desktop.control_cache_ttl_ms === undefined ||
    Number.isNaN(Number(target.desktop.control_cache_ttl_ms))
  ) {
    target.desktop.control_cache_ttl_ms = defaultDesktopConfig.control_cache_ttl_ms
  }
  target.desktop.control_cache_ttl_ms = Math.max(
    250,
    Math.min(600000, Math.trunc(target.desktop.control_cache_ttl_ms))
  )
  if (target.desktop.max_controls === undefined || Number.isNaN(Number(target.desktop.max_controls))) {
    target.desktop.max_controls = defaultDesktopConfig.max_controls
  }
  target.desktop.max_controls = Math.max(10, Math.min(10000, Math.trunc(target.desktop.max_controls)))
  if (target.desktop.screenshot_keep_count === undefined || Number.isNaN(Number(target.desktop.screenshot_keep_count))) {
    target.desktop.screenshot_keep_count = defaultDesktopConfig.screenshot_keep_count
  }
  target.desktop.screenshot_keep_count = Math.max(
    20,
    Math.min(10000, Math.trunc(target.desktop.screenshot_keep_count))
  )
  if (!['high_risk_only', 'always_ask', 'always_allow'].includes(target.desktop.approval_mode)) {
    target.desktop.approval_mode = defaultDesktopConfig.approval_mode
  }
  if (target.desktop.screenshot_dir === undefined) {
    target.desktop.screenshot_dir = null
  }
}

function ensureAutomationConfig(target: Config) {
  if (!target.automation) {
    target.automation = deepClone(defaultAutomationConfig)
  }
  if (target.automation.enabled === undefined) {
    target.automation.enabled = defaultAutomationConfig.enabled
  }
  if (
    target.automation.max_concurrent_runs === undefined ||
    Number.isNaN(Number(target.automation.max_concurrent_runs))
  ) {
    target.automation.max_concurrent_runs = defaultAutomationConfig.max_concurrent_runs
  }
  target.automation.max_concurrent_runs = Math.max(
    1,
    Math.min(8, Math.trunc(target.automation.max_concurrent_runs))
  )
  if (!['ask', 'minimize_to_tray', 'exit'].includes(target.automation.close_behavior)) {
    target.automation.close_behavior = defaultAutomationConfig.close_behavior
  }
  if (!target.automation.heartbeat) {
    target.automation.heartbeat = deepClone(defaultAutomationConfig.heartbeat)
  }
  if (target.automation.heartbeat.enabled === undefined) {
    target.automation.heartbeat.enabled = defaultAutomationConfig.heartbeat.enabled
  }
  if (
    target.automation.heartbeat.every_minutes === undefined ||
    Number.isNaN(Number(target.automation.heartbeat.every_minutes))
  ) {
    target.automation.heartbeat.every_minutes = defaultAutomationConfig.heartbeat.every_minutes
  }
  target.automation.heartbeat.every_minutes = Math.max(
    1,
    Math.min(1440, Math.trunc(target.automation.heartbeat.every_minutes))
  )
  if (typeof target.automation.heartbeat.prompt !== 'string' || !target.automation.heartbeat.prompt.trim()) {
    target.automation.heartbeat.prompt = defaultAutomationConfig.heartbeat.prompt
  }
  if (!Array.isArray(target.automation.heartbeat.tool_whitelist)) {
    target.automation.heartbeat.tool_whitelist = [...defaultAutomationConfig.heartbeat.tool_whitelist]
  }
  target.automation.heartbeat.tool_whitelist = target.automation.heartbeat.tool_whitelist
    .map((item) => item.trim())
    .filter((item, index, self) => item.length > 0 && self.indexOf(item) === index)
}

const browserProfileNames = computed(() => {
  const browser = localConfig.value.browser
  if (!browser?.profiles) return []
  return Object.keys(browser.profiles)
})

const activeBrowserProfileName = computed({
  get: () => {
    ensureBrowserConfig(localConfig.value)
    return localConfig.value.browser.default_profile
  },
  set: (value: string) => {
    ensureBrowserConfig(localConfig.value)
    if (localConfig.value.browser.profiles[value]) {
      localConfig.value.browser.default_profile = value
    }
  }
})

const activeBrowserProfile = computed<BrowserProfileConfig>({
  get: () => {
    ensureBrowserConfig(localConfig.value)
    const name = activeBrowserProfileName.value
    return localConfig.value.browser.profiles[name]
  },
  set: (value) => {
    ensureBrowserConfig(localConfig.value)
    localConfig.value.browser.profiles[activeBrowserProfileName.value] = value
  }
})

watch(() => props.modelValue, (val) => {
  if (val) {
    const currentConfig = configStore.config as Config
    localConfig.value = {
      ...defaultConfig,
      ...currentConfig,
      mcp_servers: [...(currentConfig.mcp_servers ?? [])],
      browser: deepClone(currentConfig.browser ?? defaultBrowserConfig),
      desktop: deepClone(currentConfig.desktop ?? defaultDesktopConfig),
      automation: deepClone(currentConfig.automation ?? defaultAutomationConfig)
    }
    localConfig.value.theme = 'light'
    ensureBrowserConfig(localConfig.value)
    ensureDesktopConfig(localConfig.value)
    ensureAutomationConfig(localConfig.value)
  }
})



async function handleSave() {
  saving.value = true
  try {
    localConfig.value.theme = 'light'
    ensureBrowserConfig(localConfig.value)
    ensureDesktopConfig(localConfig.value)
    ensureAutomationConfig(localConfig.value)
    await configStore.saveConfig(localConfig.value)
    ElMessage.success('Settings saved successfully')
    dialogVisible.value = false
  } catch (error) {
    ElMessage.error('Failed to save settings')
  } finally {
    saving.value = false
  }
}

function addMcpServer() {
  if (!newMcpServer.value.name) {
    ElMessage.warning('Please enter a server name')
    return
  }

  const server: McpServerConfig = {
    name: newMcpServer.value.name,
    enabled: true,
    transport: {}
  }

  if (newMcpServer.value.transportType === 'stdio') {
    server.transport = {
      type: 'stdio',
      command: newMcpServer.value.command,
      args: newMcpServer.value.args.split(' ').filter(Boolean)
    }
  } else {
    server.transport = {
      type: 'http',
      url: newMcpServer.value.url
    }
  }

  localConfig.value.mcp_servers.push(server)
  showAddMcpDialog.value = false

  // Reset form
  newMcpServer.value = createDefaultMcpServerForm()
}

function removeMcpServer(index: number) {
  localConfig.value.mcp_servers.splice(index, 1)
}

async function openBrowserProfileDir() {
  try {
    const profilePath = await invoke<string>('open_browser_profile_dir', {
      profile: activeBrowserProfileName.value
    })
    ElMessage.success(`Opened: ${profilePath}`)
  } catch (error) {
    ElMessage.error('Failed to open browser profile directory')
  }
}

async function resetBrowserProfile() {
  try {
    await ElMessageBox.confirm(
      `This will delete profile "${activeBrowserProfileName.value}" browser data (cookies/session). Continue?`,
      'Reset Browser Profile',
      {
        type: 'warning',
        confirmButtonText: 'Reset',
        cancelButtonText: 'Cancel'
      }
    )
    await invoke('reset_browser_profile', {
      profile: activeBrowserProfileName.value
    })
    ElMessage.success('Browser profile reset complete')
  } catch (error) {
    // user cancelled or reset failed
    if (error !== 'cancel') {
      ElMessage.error('Failed to reset browser profile')
    }
  }
}
</script>

<style scoped>
.mcp-list {
  max-height: 400px;
  overflow-y: auto;
}

.mcp-item {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 12px;
  border: 1px solid var(--color-border);
  border-radius: 6px;
  margin-bottom: 8px;
}

.mcp-info {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.mcp-type {
  font-size: 12px;
  color: var(--color-text-secondary);
}

.mcp-actions {
  display: flex;
  align-items: center;
  gap: 12px;
}

.viewport-row {
  display: flex;
  align-items: center;
  gap: 8px;
}

.viewport-sep {
  color: var(--color-text-secondary);
}

.browser-actions {
  display: flex;
  gap: 8px;
}

.setting-hint {
  margin-left: 10px;
  font-size: 12px;
  color: var(--color-text-secondary);
  line-height: 1.4;
}
</style>


