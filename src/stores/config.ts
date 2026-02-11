import { defineStore } from 'pinia'
import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'

export interface Config {
  api_key?: string
  api_base?: string
  model: string
  system_prompt?: string
  work_directory?: string
  theme: string
  mcp_servers: McpServerConfig[]
}

export interface McpServerConfig {
  name: string
  transport: any
  enabled: boolean
}

export const useConfigStore = defineStore('config', () => {
  const config = ref<Config>({
    api_base: 'https://open.bigmodel.cn/api/paas/v4',
    model: 'glm-4.7',
    system_prompt: '',
    theme: 'dark',
    mcp_servers: []
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
