<template>
  <div class="account-stack">
    <!-- 加载状态 -->
    <div v-if="loading" class="account-loading">
      <span class="material-icons-round spin">sync</span>
      <span>加载中...</span>
    </div>

    <!-- 未登录状态 -->
    <div v-else-if="!isLoggedIn" class="account-card login-prompt">
      <span class="material-icons-round" style="font-size: 48px; color: var(--accent)">account_circle</span>
      <p>请先登录以查看账户信息</p>
      <button class="account-btn primary" type="button" @click="emit('open-login')">
        立即登录
      </button>
    </div>

    <!-- 已登录：资料展示 -->
    <template v-else-if="profile">
      <div class="account-card profile-hero">
        <div class="profile-avatar-wrap">
          <img :src="profile.avatar || defaultAvatar" alt="User Avatar" class="profile-avatar" />
        </div>
        <div class="profile-main">
          <div class="profile-title-row">
            <h2>{{ profile.username }}</h2>
            <span class="profile-plan-tag">{{ planLabel }}</span>
          </div>
          <p class="profile-email">{{ profile.email }}</p>
          <div class="profile-actions">
            <button class="account-btn ghost" type="button" @click="emit('open-login')">切换账号</button>
          </div>
        </div>
      </div>

      <div class="account-two-col">
        <div class="account-card membership-card">
          <div class="membership-label">会员等级</div>
          <div class="membership-value">{{ levelLabel }}</div>
          <div class="membership-desc">{{ membershipDesc }}</div>
        </div>
        <div class="account-card token-card">
          <div class="token-label">剩余 AI 额度</div>
          <div class="token-value">
            {{ profile.token_balance.toLocaleString() }}
            <span>tokens</span>
          </div>
          <div class="token-progress">
            <div class="token-progress-bar" :style="{ width: `${profile.token_usage_percent}%` }"></div>
          </div>
        </div>
      </div>

      <div v-if="profile.membership_expire_at" class="account-card expiry-card">
        <div>
          <div class="expiry-label">账户有效期</div>
          <div class="expiry-value">
            {{ formatExpiry(profile.membership_expire_at) }}
            <span class="expiry-status" :class="{ warning: profile.days_left <= 7 }">
              {{ profile.days_left > 0 ? '状态正常' : '已到期' }}
            </span>
          </div>
          <p class="expiry-tip">距离到期还有 {{ profile.days_left }} 天</p>
        </div>
        <div class="expiry-icon">
          <span class="material-icons-round">event_available</span>
        </div>
        <div class="expiry-footer">
          <button class="account-btn primary" type="button" @click="emit('open-renew')">
            <span>立即续费</span>
            <span class="material-icons-round">arrow_forward</span>
          </button>
        </div>
      </div>

      <!-- 未开通会员 -->
      <div v-else class="account-card expiry-card">
        <div>
          <div class="expiry-label">会员状态</div>
          <div class="expiry-value" style="color: var(--text-secondary)">未开通会员</div>
          <p class="expiry-tip">开通会员享受更多权益</p>
        </div>
        <div class="expiry-footer">
          <button class="account-btn primary" type="button" @click="emit('open-renew')">
            <span>立即开通</span>
            <span class="material-icons-round">arrow_forward</span>
          </button>
        </div>
      </div>
    </template>

    <!-- 加载失败 -->
    <div v-else class="account-card error-card">
      <span class="material-icons-round" style="color: var(--error)">error_outline</span>
      <p>{{ error || '加载失败' }}</p>
      <button class="account-btn secondary" type="button" @click="loadProfile">重试</button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted, computed } from 'vue'
import { invoke } from '@tauri-apps/api/core'

const emit = defineEmits<{
  (event: 'open-renew'): void
  (event: 'open-login'): void
}>()

interface UserProfile {
  user_id: string
  username: string
  email: string
  avatar: string | null
  membership_level: string
  membership_expire_at: string | null
  days_left: number
  token_balance: number
  token_total_used: number
  token_usage_percent: number
}

const loading = ref(true)
const isLoggedIn = ref(false)
const profile = ref<UserProfile | null>(null)
const error = ref('')
const defaultAvatar = 'https://api.dicebear.com/7.x/bottts/svg?seed=petool'

const planLabel = computed(() => {
  const level = profile.value?.membership_level
  if (level === 'pro') return 'Pro Plan'
  if (level === 'enterprise') return 'Enterprise'
  return 'Free Plan'
})

const levelLabel = computed(() => {
  const level = profile.value?.membership_level
  if (level === 'pro') return '专业会员'
  if (level === 'enterprise') return '企业会员'
  return '免费用户'
})

const membershipDesc = computed(() => {
  if (profile.value?.membership_level === 'free') return '升级享受更多权益'
  return '高级会员尊享权益生效中'
})

function formatExpiry(dateStr: string | null): string {
  if (!dateStr) return '--'
  const d = new Date(dateStr)
  return `${d.getFullYear()}年${d.getMonth() + 1}月${d.getDate()}日`
}

async function loadProfile() {
  loading.value = true
  error.value = ''
  try {
    isLoggedIn.value = await invoke<boolean>('petool_is_logged_in')
    if (!isLoggedIn.value) return
    profile.value = await invoke<UserProfile>('petool_get_profile')
  } catch (e: any) {
    error.value = e?.toString() || '未知错误'
    isLoggedIn.value = false
  } finally {
    loading.value = false
  }
}

onMounted(() => {
  void loadProfile()
})
</script>
