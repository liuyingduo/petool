import { defineStore } from 'pinia'
import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'

export interface Config {
  api_key?: string
  api_base?: string
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
  browser: BrowserConfig
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
