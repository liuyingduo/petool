<template>
  <div class="settings-shell-page">
    <div class="settings-bg" aria-hidden="true">
      <div class="blob blob-a"></div>
      <div class="blob blob-b"></div>
    </div>

    <main class="settings-shell">
      <div class="shell-eyes" aria-hidden="true">
        <div class="shell-eye"><div class="eye-pupil"></div></div>
        <div class="shell-eye"><div class="eye-pupil"></div></div>
      </div>

      <div class="shell-window-controls" role="group" aria-label="窗口控制">
        <button class="shell-control" type="button" title="最小化" @click="handleMinimize">
          <span class="material-icons-round">remove</span>
        </button>
        <button
          class="shell-control"
          type="button"
          :title="isWindowMaximized ? '还原' : '最大化'"
          @click="handleToggleMaximize"
        >
          <span class="material-icons-round">{{ isWindowMaximized ? 'filter_none' : 'check_box_outline_blank' }}</span>
        </button>
        <button class="shell-control close" type="button" title="关闭" @click="handleClose">
          <span class="material-icons-round">close</span>
        </button>
      </div>

      <aside class="settings-sidebar">
        <div class="sidebar-main">
          <button class="back-home-btn" type="button" @click="goHome">
            <span class="material-symbols-outlined">arrow_back</span>
            <span>返回主页</span>
          </button>

          <div class="sidebar-divider"></div>
          <div class="sidebar-title">设置选项</div>

          <div class="sidebar-nav">
            <button
              v-for="item in sectionItems"
              :key="item.key"
              class="settings-nav-item"
              :class="{ active: activeSection === item.key }"
              type="button"
              @click="goSection(item.key)"
            >
              <span class="material-icons-round">{{ item.icon }}</span>
              <span>{{ item.label }}</span>
            </button>
          </div>
        </div>

        <div class="sidebar-user-card">
          <div class="sidebar-user-meta">
            <div class="sidebar-avatar">
              <img
                alt="User"
                src="https://lh3.googleusercontent.com/aida-public/AB6AXuBYaZM97JogdW-ya3ULqGOtiyNOHmX7QgQJQ1c7qMdDxTpN__9ZBn0Jq6D5AQiHwClbXSmKaP3yFa-GzJuTHIsZ6OObIjCQ9QHApIpAuKMYIWptOHH6KVzLGp4nU5DO48mIg48o3YedtwFShv6G0Tq-ir30SVT7WgAWCksaPf_PnwnEwCx7rOimt23ZlQC3VUyfRbucQrEvpTkLIEwEwiWZ_gSWFyekl4IxXUqKEUqrS2CVHHlvuJqUmCJBLBYKUuDKiuQqkueqB3Y"
              >
            </div>
            <div class="sidebar-user-text">
              <span class="name">Alex</span>
              <span class="plan">Pro Plan</span>
            </div>
          </div>
          <button class="sidebar-settings-btn" type="button" aria-label="设置">
            <span class="material-icons-round">settings</span>
          </button>
        </div>
      </aside>

      <section class="settings-content">
        <header class="settings-header">
          <h1>{{ headerTitle }}</h1>
          <p>{{ headerDesc }}</p>
        </header>

        <div class="settings-content-body no-scrollbar">
          <GeneralSettingsPage v-if="activeSection === 'general'" />
          <NotificationSettingsPage v-else-if="activeSection === 'notifications'" />
          <template v-else-if="activeSection === 'about'">
            <FeedbackPage v-if="aboutPage === 'feedback'" />
            <TutorialPage v-else-if="aboutPage === 'tutorial'" />
            <AgreementPage v-else-if="aboutPage === 'agreement'" />
            <AboutPetoolPage v-else @open-page="goAboutPage" />
          </template>
          <AdvancedSettingsPage v-else />
        </div>
      </section>
    </main>
  </div>
</template>

<script setup lang="ts">
import { computed, onMounted, ref } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { invoke } from '@tauri-apps/api/core'
import { useConfigStore } from '@/stores/config'
import GeneralSettingsPage from './settings/GeneralSettingsPage.vue'
import NotificationSettingsPage from './settings/NotificationSettingsPage.vue'
import AboutPetoolPage from './settings/AboutPetoolPage.vue'
import AdvancedSettingsPage from './settings/AdvancedSettingsPage.vue'
import FeedbackPage from './settings/FeedbackPage.vue'
import TutorialPage from './settings/TutorialPage.vue'
import AgreementPage from './settings/AgreementPage.vue'

type SettingsSection = 'general' | 'notifications' | 'about' | 'advanced'

const route = useRoute()
const router = useRouter()
const appWindow = getCurrentWindow()
const isWindowMaximized = ref(false)
const configStore = useConfigStore()

const sectionItems: Array<{ key: SettingsSection; label: string; icon: string }> = [
  { key: 'general', label: '通用设置', icon: 'tune' },
  { key: 'notifications', label: '通知管理', icon: 'notifications' },
  { key: 'about', label: '关于 Petool', icon: 'info' },
  { key: 'advanced', label: '高级设置', icon: 'engineering' }
]

const activeSection = computed<SettingsSection>(() => {
  const raw = String(route.params.section || '')
  if (raw === 'general' || raw === 'notifications' || raw === 'about' || raw === 'advanced') {
    return raw
  }
  if (route.path.startsWith('/settings/about/')) return 'about'
  return 'general'
})

const aboutPage = computed(() => {
  const raw = String(route.params.page || '')
  return raw || null
})

const headerTitle = computed(() => {
  if (activeSection.value === 'general') return '通用设置'
  if (activeSection.value === 'notifications') return '通知管理'
  if (activeSection.value === 'about') {
    if (aboutPage.value === 'feedback') return '反馈问题'
    if (aboutPage.value === 'tutorial') return '使用教程'
    if (aboutPage.value === 'agreement') return '用户协议'
    return '关于 Petool'
  }
  return '高级设置'
})

const headerDesc = computed(() => {
  if (activeSection.value === 'general') return '管理 Petool 的基础行为和偏好'
  if (activeSection.value === 'notifications') return '自定义 Petool 的提醒方式'
  if (activeSection.value === 'about') return '版本信息、帮助文档与反馈入口'
  return '模型、Browser、Desktop、Automation 与 MCP 深度配置'
})

function goHome() {
  void router.push('/')
}

function goSection(section: SettingsSection) {
  if (section === 'about') {
    void router.push('/settings/about')
    return
  }
  void router.push(`/settings/${section}`)
}

function goAboutPage(page: 'feedback' | 'tutorial' | 'agreement') {
  void router.push(`/settings/about/${page}`)
}

onMounted(() => {
  void configStore.loadConfig()
})

async function handleMinimize() {
  try {
    await appWindow.minimize()
  } catch {
    // ignore
  }
}

async function handleToggleMaximize() {
  try {
    await appWindow.toggleMaximize()
    isWindowMaximized.value = await appWindow.isMaximized()
  } catch {
    // ignore
  }
}

async function handleClose() {
  try {
    await invoke('app_exit_now')
  } catch {
    // ignore
  }
}
</script>

<style src="@/styles/settings-shell.css"></style>
