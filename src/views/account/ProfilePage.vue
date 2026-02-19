<template>
  <div class="account-stack">
    <div class="account-card profile-hero">
      <div class="profile-avatar-wrap">
        <img :src="profile.avatar" alt="User Avatar" class="profile-avatar" />
        <button class="profile-avatar-action" type="button" title="更改头像">
          <span class="material-icons-round">camera_alt</span>
        </button>
      </div>
      <div class="profile-main">
        <div class="profile-title-row">
          <h2>{{ profile.name }}</h2>
          <span class="profile-plan-tag">{{ profile.plan }}</span>
        </div>
        <p class="profile-email">{{ profile.email }}</p>
        <div class="profile-actions">
          <button class="account-btn secondary" type="button">编辑资料</button>
          <button class="account-btn ghost" type="button">修改密码</button>
        </div>
      </div>
    </div>

    <div class="account-two-col">
      <div class="account-card membership-card">
        <div class="membership-label">会员等级</div>
        <div class="membership-value">{{ profile.level }}</div>
        <div class="membership-desc">高级会员尊享权益生效中</div>
      </div>
      <div class="account-card token-card">
        <div class="token-label">剩余 AI 额度</div>
        <div class="token-value">
          {{ profile.tokensRemaining.toLocaleString() }}
          <span>tokens</span>
        </div>
        <div class="token-progress">
          <div class="token-progress-bar" :style="{ width: `${profile.tokenUsagePercent}%` }"></div>
        </div>
      </div>
    </div>

    <div class="account-card expiry-card">
      <div>
        <div class="expiry-label">账户有效期</div>
        <div class="expiry-value">
          {{ profile.expirationDate }}
          <span class="expiry-status">状态正常</span>
        </div>
        <p class="expiry-tip">距离到期还有 {{ profile.daysLeft }} 天</p>
      </div>
      <div class="expiry-icon">
        <span class="material-icons-round">event_available</span>
      </div>
      <div class="expiry-footer">
        <p>自动续费已开启，将于到期前 24 小时扣费。</p>
        <button class="account-btn primary" type="button" @click="emit('open-renew')">
          <span>立即续费</span>
          <span class="material-icons-round">arrow_forward</span>
        </button>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { accountProfile as profile } from './mock'

const emit = defineEmits<{
  (event: 'open-renew'): void
}>()
</script>
