import { defineStore } from 'pinia'
import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'

export interface Config {
  api_key?: string
  api_base?: string
  model: string
  system_prompt?: string
  work_directory?: string
  conversation_workspaces: Record<string, string>
  theme: string
  tool_display_mode: ToolDisplayMode
  mcp_servers: McpServerConfig[]
  tool_permissions: Record<string, ToolPermissionAction>
  tool_path_permissions: ToolPathPermissionRule[]
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
  operation_timeout_ms: number
  profiles: Record<string, BrowserProfileConfig>
}

export const useConfigStore = defineStore('config', () => {
  const config = ref<Config>({
    api_base: 'https://open.bigmodel.cn/api/paas/v4',
    model: 'glm-5',
    system_prompt: '',
    conversation_workspaces: {},
    theme: 'dark',
    tool_display_mode: 'compact',
    mcp_servers: [],
    tool_permissions: {},
    tool_path_permissions: [],
    browser: {
      enabled: true,
      default_profile: 'openclaw',
      evaluate_enabled: false,
      allow_private_network: false,
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
