<template>
  <div class="settings-stack">
    <div class="settings-card">
      <div class="setting-item">
        <div class="setting-head-left">
          <div class="setting-icon blue">
            <span class="material-icons-round">power_settings_new</span>
          </div>
          <div class="setting-item-body">
            <h3 class="setting-item-title">开机自动开启</h3>
            <p class="setting-item-desc">启用后，Petool 将随系统启动并在后台运行。</p>
          </div>
        </div>
        <input v-model="localConfig.autostart_enabled" class="simple-toggle" type="checkbox" />
      </div>
    </div>

    <div class="settings-card">
      <div class="setting-item">
        <div class="setting-head-left">
          <div class="setting-icon green">
            <span class="material-icons-round">auto_fix_high</span>
          </div>
          <div class="setting-item-body">
            <h3 class="setting-item-title">自动执行工具开启</h3>
            <p class="setting-item-desc">开启后，AI 调用功能时不再弹窗询问，直接自动运行。</p>
          </div>
        </div>
        <input v-model="localConfig.auto_approve_tool_requests" class="simple-toggle" type="checkbox" />
      </div>
    </div>

    <div class="settings-card">
      <div class="setting-card-head">
        <div class="setting-head-left">
          <div class="setting-icon orange">
            <span class="material-icons-round">file_download</span>
          </div>
          <div>
            <h3 class="setting-title">下载默认保存路径</h3>
            <p class="setting-desc">下载缓存、技能和运行时安装统一在该目录下。</p>
          </div>
        </div>
      </div>
      <div class="path-grid">
        <input
          v-model="downloadsDirectoryText"
          class="field-input"
          readonly
          placeholder="请选择下载目录"
          type="text"
        />
        <button class="btn secondary" type="button" @click="selectDownloadsDirectory">更改</button>
      </div>
    </div>

    <div class="settings-card">
      <div class="setting-card-head">
        <div class="setting-head-left">
          <div class="setting-icon indigo">
            <span class="material-icons-round">work_outline</span>
          </div>
          <div>
            <h3 class="setting-title">工作默认路径</h3>
            <p class="setting-desc">设置助手执行自动化任务时的基础工作目录。</p>
          </div>
        </div>
      </div>
      <div class="path-grid">
        <input
          v-model="workDirectoryText"
          class="field-input"
          readonly
          placeholder="请选择工作目录"
          type="text"
        />
        <button class="btn secondary" type="button" @click="selectWorkDirectory">选择</button>
      </div>
    </div>

    <div class="btn-row">
      <button class="btn primary" :disabled="saving" type="button" @click="save">
        {{ saving ? '保存中...' : '保存通用设置' }}
      </button>
    </div>

    <div v-if="status.text" class="status-chip" :class="status.type">{{ status.text }}</div>

    <div class="meta-footer">Petool v2.4.0 (Build 20231024)</div>
  </div>
</template>

<script setup lang="ts">
import { computed, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { useConfigStore, type Config } from '@/stores/config'

const configStore = useConfigStore()
const saving = ref(false)
const status = ref<{ type: 'success' | 'error' | 'info'; text: string }>({ type: 'info', text: '' })

function deepClone<T>(value: T): T {
  return JSON.parse(JSON.stringify(value)) as T
}

const localConfig = ref<Config>(deepClone(configStore.config))

const downloadsDirectoryText = computed({
  get: () => localConfig.value.downloads_directory || '',
  set: (value: string) => {
    localConfig.value.downloads_directory = value || null
  }
})

const workDirectoryText = computed({
  get: () => localConfig.value.work_directory || '',
  set: (value: string) => {
    localConfig.value.work_directory = value
  }
})

watch(
  () => configStore.config,
  (value) => {
    localConfig.value = deepClone(value)
  },
  { deep: true }
)

async function selectWorkDirectory() {
  try {
    const path = await invoke<string | null>('select_folder')
    if (path) {
      localConfig.value.work_directory = path
      status.value = { type: 'info', text: '' }
    }
  } catch {
    status.value = { type: 'error', text: '选择工作目录失败。' }
  }
}

async function selectDownloadsDirectory() {
  try {
    const path = await invoke<string | null>('select_folder')
    if (path) {
      localConfig.value.downloads_directory = path
      status.value = { type: 'info', text: '' }
    }
  } catch {
    status.value = { type: 'error', text: '选择下载目录失败。' }
  }
}

async function save() {
  saving.value = true
  status.value = { type: 'info', text: '' }
  try {
    await configStore.saveConfig(localConfig.value)
    await configStore.loadConfig()
    localConfig.value = deepClone(configStore.config)
    status.value = { type: 'success', text: '通用设置已保存。' }
  } catch (error) {
    status.value = { type: 'error', text: typeof error === 'string' ? error : '保存失败，请重试。' }
  } finally {
    saving.value = false
  }
}
</script>
