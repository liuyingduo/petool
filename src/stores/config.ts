import { defineStore } from 'pinia'
import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'

export interface Config {
  api_key?: string
  api_base?: string
  petool_token?: string
  petool_api_base?: string
  clawhub_api_key?: string
  clawhub_api_base?: string
  ark_api_key?: string
  ark_api_base?: string
  minimax_api_key?: string
  image_model: string
  image_understand_model: string
  video_model: string
  image_size: string
  image_watermark: boolean
  model: string
  system_prompt?: string
  work_directory?: string
  conversation_workspaces: Record<string, string>
  theme: string
  tool_display_mode: ToolDisplayMode
  mcp_servers: McpServerConfig[]
  tool_permissions: Record<string, ToolPermissionAction>
  tool_path_permissions: ToolPathPermissionRule[]
  auto_approve_tool_requests: boolean
  autostart_enabled: boolean
  downloads_directory?: string | null
  notifications: NotificationSettings
  browser: BrowserConfig
  desktop: DesktopConfig
  automation: AutomationConfig
}

export interface NotificationSettings {
  sound_enabled: boolean
  break_reminder_enabled: boolean
  task_completed_enabled: boolean
}

export type ToolPermissionAction = 'allow' | 'ask' | 'deny'
export type ToolDisplayMode = 'compact' | 'full'

export interface ToolPathPermissionRule {
  tool_pattern: string
  path_pattern: string
  action: ToolPermissionAction
}

export interface McpServerConfig {
  name: string
  transport: any
  enabled: boolean
}

export type BrowserEngine = 'chromium' | 'chrome'

export interface BrowserViewport {
  width: number
  height: number
}

export interface BrowserProfileConfig {
  engine: BrowserEngine
  headless: boolean
  executable_path?: string | null
  cdp_url?: string | null
  user_data_dir?: string | null
  color: string
  viewport: BrowserViewport
}

export interface BrowserConfig {
  enabled: boolean
  default_profile: string
  evaluate_enabled: boolean
  allow_private_network: boolean
  performance_preset: 'safe' | 'balanced' | 'fast'
  capture_response_bodies: boolean
  default_act_timeout_ms: number
  operation_timeout_ms: number
  profiles: Record<string, BrowserProfileConfig>
}

export type DesktopApprovalMode = 'high_risk_only' | 'always_ask' | 'always_allow'

export interface DesktopConfig {
  enabled: boolean
  operation_timeout_ms: number
  control_cache_ttl_ms: number
  max_controls: number
  screenshot_dir?: string | null
  screenshot_keep_count: number
  approval_mode: DesktopApprovalMode
}

export type AutomationCloseBehavior = 'ask' | 'minimize_to_tray' | 'exit'

export interface HeartbeatAutomationConfig {
  enabled: boolean
  every_minutes: number
  target_conversation_id?: string | null
  prompt: string
  model?: string | null
  workspace_directory?: string | null
  tool_whitelist: string[]
}

export interface AutomationConfig {
  enabled: boolean
  max_concurrent_runs: number
  close_behavior: AutomationCloseBehavior
  heartbeat: HeartbeatAutomationConfig
}

export const useConfigStore = defineStore('config', () => {
  const config = ref<Config>({
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
    browser: {
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
    },
    desktop: {
      enabled: true,
      operation_timeout_ms: 20000,
      control_cache_ttl_ms: 120000,
      max_controls: 800,
      screenshot_dir: null,
      screenshot_keep_count: 200,
      approval_mode: 'high_risk_only'
    },
    automation: {
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
  })

  const loading = ref(false)

  async function loadConfig() {
    loading.value = true
    try {
      config.value = await invoke<Config>('get_config')
    } catch (error) {
      console.error('Failed to load config:', error)
    } finally {
      loading.value = false
    }
  }

  async function saveConfig(newConfig: Config) {
    try {
      await invoke('set_config', { config: newConfig })
      config.value = newConfig
    } catch (error) {
      console.error('Failed to save config:', error)
      throw error
    }
  }

  async function validateApiKey(apiKey: string, apiBase?: string) {
    try {
      return await invoke<boolean>('validate_api_key', { apiKey, apiBase })
    } catch (error) {
      console.error('Failed to validate API key:', error)
      return false
    }
  }

  return {
    config,
    loading,
    loadConfig,
    saveConfig,
    validateApiKey
  }
})

