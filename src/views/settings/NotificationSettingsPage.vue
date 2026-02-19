<template>
  <div class="settings-stack">
    <div class="settings-card">
      <div class="setting-item">
        <div class="setting-head-left">
          <div class="setting-icon purple">
            <span class="material-icons-round">music_note</span>
          </div>
          <div class="setting-item-body">
            <h3 class="setting-item-title">声音提醒</h3>
            <p class="setting-item-desc">开启 8-bit 复古宠物音效，与你的 Petool 互动。</p>
          </div>
        </div>
        <input v-model="localNotifications.sound_enabled" class="simple-toggle" type="checkbox" />
      </div>
    </div>

    <div class="settings-card">
      <div class="setting-item">
        <div class="setting-head-left">
          <div class="setting-icon amber">
            <span class="material-icons-round">coffee</span>
          </div>
          <div class="setting-item-body">
            <h3 class="setting-item-title">摸鱼休息提醒</h3>
            <p class="setting-item-desc">工作太久了吗？让 Petool 定时提醒你喝水或休息一下。</p>
          </div>
        </div>
        <input v-model="localNotifications.break_reminder_enabled" class="simple-toggle" type="checkbox" />
      </div>
    </div>

    <div class="settings-card">
      <div class="setting-item">
        <div class="setting-head-left">
          <div class="setting-icon green">
            <span class="material-icons-round">task_alt</span>
          </div>
          <div class="setting-item-body">
            <h3 class="setting-item-title">任务完成通知</h3>
            <p class="setting-item-desc">当文件夹扫描或文件处理完成时接收即时通知。</p>
          </div>
        </div>
        <input v-model="localNotifications.task_completed_enabled" class="simple-toggle" type="checkbox" />
      </div>
    </div>

    <div class="btn-row">
      <button class="btn primary" :disabled="saving" type="button" @click="save">
        <span>{{ saving ? '保存中...' : '保存通知设置' }}</span>
      </button>
    </div>

    <div v-if="status.text" class="status-chip" :class="status.type">{{ status.text }}</div>

    <div class="meta-footer">Petool v2.4.0 (Build 20231024)</div>
  </div>
</template>

<script setup lang="ts">
import { ref, watch } from 'vue'
import { useConfigStore, type NotificationSettings } from '@/stores/config'

const configStore = useConfigStore()
const saving = ref(false)
const status = ref<{ type: 'success' | 'error' | 'info'; text: string }>({ type: 'info', text: '' })

const localNotifications = ref<NotificationSettings>({
  sound_enabled: false,
  break_reminder_enabled: true,
  task_completed_enabled: true
})

watch(
  () => configStore.config.notifications,
  (value) => {
    localNotifications.value = {
      sound_enabled: value?.sound_enabled ?? false,
      break_reminder_enabled: value?.break_reminder_enabled ?? true,
      task_completed_enabled: value?.task_completed_enabled ?? true
    }
  },
  { immediate: true, deep: true }
)

async function save() {
  saving.value = true
  status.value = { type: 'info', text: '' }
  try {
    await configStore.saveConfig({
      ...configStore.config,
      notifications: { ...localNotifications.value }
    })
    await configStore.loadConfig()
    status.value = { type: 'success', text: '通知设置已保存。' }
  } catch (error) {
    status.value = { type: 'error', text: typeof error === 'string' ? error : '保存失败，请重试。' }
  } finally {
    saving.value = false
  }
}
</script>
