<template>
  <div class="settings-stack">
    <div class="settings-card">
      <div class="setting-card-head">
        <div class="setting-head-left">
          <div class="setting-icon indigo">
            <span class="material-icons-round">tune</span>
          </div>
          <div>
            <h3 class="setting-title">高级设置中心</h3>
            <p class="setting-desc">模型、Browser、Desktop、Automation、MCP 等能力的深度配置入口。</p>
          </div>
        </div>
      </div>

      <div class="btn-row" style="margin-top: 16px;">
        <button class="btn primary" type="button" @click="showLegacy = true">打开完整高级设置面板</button>
        <button class="btn secondary" type="button" @click="reloadConfig">刷新配置</button>
      </div>

      <div v-if="status.text" class="status-chip" :class="status.type">{{ status.text }}</div>
    </div>

    <div class="settings-card compact">
      <h3 class="settings-section-title">当前关键配置摘要</h3>
      <p class="settings-section-desc">用于快速确认核心状态，详细修改请打开完整高级设置面板。</p>

      <div class="advanced-grid" style="margin-top: 12px;">
        <div class="advanced-kv">
          <span class="k">文本模型</span>
          <span class="v">{{ configStore.config.model }}</span>
        </div>
        <div class="advanced-kv">
          <span class="k">工作目录</span>
          <span class="v">{{ configStore.config.work_directory || '未设置' }}</span>
        </div>
        <div class="advanced-kv">
          <span class="k">自动执行工具</span>
          <span class="v">{{ configStore.config.auto_approve_tool_requests ? '开启' : '关闭' }}</span>
        </div>
        <div class="advanced-kv">
          <span class="k">Browser Tool</span>
          <span class="v">{{ configStore.config.browser?.enabled ? '开启' : '关闭' }}</span>
        </div>
        <div class="advanced-kv">
          <span class="k">Desktop Tool</span>
          <span class="v">{{ configStore.config.desktop?.enabled ? '开启' : '关闭' }}</span>
        </div>
        <div class="advanced-kv">
          <span class="k">MCP 服务器数量</span>
          <span class="v">{{ configStore.config.mcp_servers?.length ?? 0 }}</span>
        </div>
      </div>
    </div>

    <SettingsDialog v-model="showLegacy" />
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { useConfigStore } from '@/stores/config'
import SettingsDialog from '@/components/Settings/index.vue'

const showLegacy = ref(false)
const configStore = useConfigStore()
const status = ref<{ type: 'success' | 'error' | 'info'; text: string }>({ type: 'info', text: '' })

async function reloadConfig() {
  try {
    await configStore.loadConfig()
    status.value = { type: 'success', text: '配置已刷新。' }
  } catch {
    status.value = { type: 'error', text: '刷新失败，请稍后重试。' }
  }
}
</script>
