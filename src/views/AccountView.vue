<template>
  <div class="account-shell-page">
    <div class="account-bg" aria-hidden="true">
      <div class="account-blob account-blob-a"></div>
      <div class="account-blob account-blob-b"></div>
    </div>

    <main class="account-shell">
      <div class="account-eyes" aria-hidden="true">
        <div class="account-eye"><div class="account-pupil"></div></div>
        <div class="account-eye"><div class="account-pupil"></div></div>
      </div>

      <div class="account-window-controls" role="group" aria-label="窗口控制">
        <button class="account-window-btn" type="button" title="最小化" @click="handleMinimize">
          <span class="material-icons-round">remove</span>
        </button>
        <button
          class="account-window-btn"
          type="button"
          :title="isWindowMaximized ? '还原' : '最大化'"
          @click="handleToggleMaximize"
        >
          <span class="material-icons-round">{{ isWindowMaximized ? 'filter_none' : 'check_box_outline_blank' }}</span>
        </button>
        <button class="account-window-btn close" type="button" title="关闭" @click="handleClose">
          <span class="material-icons-round">close</span>
        </button>
      </div>

      <aside class="account-sidebar">
        <button class="account-back-btn" type="button" @click="goHome">
          <span class="material-symbols-outlined">arrow_back</span>
          <span>返回聊天</span>
        </button>

        <div class="account-sidebar-divider"></div>
        <div class="account-sidebar-title">账户设置</div>

        <div class="account-nav">
          <button
            v-for="item in sectionItems"
            :key="item.key"
            class="account-nav-item"
            :class="{ active: activeSection === item.key }"
            type="button"
            @click="goSection(item.key)"
          >
            <span class="material-icons-round">{{ item.icon }}</span>
            <span>{{ item.label }}</span>
          </button>
        </div>

        <div class="account-logout-wrap">
          <button class="account-logout-btn" type="button" @click="handleLogout">
            <span class="material-icons-round">logout</span>
            <span>退出登录</span>
          </button>
        </div>
      </aside>

      <section class="account-content no-scrollbar">
        <ProfilePage v-if="activeSection === 'profile'" @open-renew="goSection('renew')" />
        <RenewPage v-else-if="activeSection === 'renew'" />
        <OrdersPage v-else-if="activeSection === 'orders'" />
        <QuotaPage v-else />
      </section>
    </main>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { useRoute, useRouter } from 'vue-router'
import { invoke } from '@tauri-apps/api/core'
import { useWindowControls } from '@/composables/useWindowControls'
import { useChatStore } from '@/stores/chat'
import ProfilePage from './account/ProfilePage.vue'
import RenewPage from './account/RenewPage.vue'
import OrdersPage from './account/OrdersPage.vue'
import QuotaPage from './account/QuotaPage.vue'

type AccountSection = 'profile' | 'renew' | 'orders' | 'quota'

const route = useRoute()
const router = useRouter()
const chatStore = useChatStore()
const { isWindowMaximized, handleMinimize, handleToggleMaximize, handleClose } = useWindowControls()

const sectionItems: Array<{ key: AccountSection; label: string; icon: string }> = [
  { key: 'profile', label: '个人资料', icon: 'person' },
  { key: 'renew', label: '立即续费', icon: 'diamond' },
  { key: 'orders', label: '订单管理', icon: 'receipt_long' },
  { key: 'quota', label: '额度管理', icon: 'data_usage' }
]

const activeSection = computed<AccountSection>(() => {
  const raw = String(route.params.section || '')
  if (raw === 'profile' || raw === 'renew' || raw === 'orders' || raw === 'quota') {
    return raw
  }
  return 'profile'
})

function goHome() {
  void router.push('/')
}

function goSection(section: AccountSection) {
  void router.push(`/account/${section}`)
}

async function handleLogout() {
  try {
    await invoke('petool_logout')
    chatStore.resetState()
  } catch {
    // ignore
  }
  void router.replace('/login')
}
</script>

<style src="@/styles/account-shell.css"></style>
