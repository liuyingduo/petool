<template>
  <div class="account-stack">
    <!-- 订阅套餐 -->
    <section class="account-section">
      <div class="section-title-row">
        <h2><span class="material-icons-round">card_membership</span>订阅会员</h2>
      </div>
      <div class="plan-grid">
        <article
          v-for="plan in membershipPlans"
          :key="plan.id"
          class="account-card plan-card"
          :class="{ featured: plan.featured }"
        >
          <div v-if="plan.badge" class="plan-badge">{{ plan.badge }}</div>
          <div class="plan-head">
            <h3>{{ plan.name }}</h3>
            <div class="plan-price">
              <span>{{ plan.priceLabel }}</span>
              <small>{{ plan.priceUnit }}</small>
            </div>
            <div v-if="plan.originalPrice" class="plan-origin">{{ plan.originalPrice }}</div>
          </div>
          <ul class="plan-feature-list">
            <li v-for="feature in plan.features" :key="feature">
              <span class="material-icons-round">{{ plan.featured ? 'verified' : 'check_circle' }}</span>
              <span>{{ feature }}</span>
            </li>
          </ul>
          <button
            class="account-btn"
            :class="plan.featured ? 'primary' : 'secondary'"
            type="button"
            :disabled="ordering === plan.id"
            @click="openPaymentDialog(plan.id)"
          >
            {{ ordering === plan.id ? '处理中...' : '立即开通' }}
          </button>
        </article>
      </div>
    </section>

    <!-- Token 加油包 -->
    <section class="account-section">
      <div class="section-title-row">
        <h2><span class="material-icons-round">bolt</span>流量加油包</h2>
      </div>
      <div class="pack-grid">
        <article
          v-for="pack in tokenPacks"
          :key="pack.id"
          class="account-card pack-card"
          :class="{ unavailable: !pack.available }"
        >
          <div>
            <div class="pack-name">{{ pack.name }}</div>
            <div class="pack-desc">{{ pack.desc }}</div>
            <div class="pack-price">{{ pack.priceLabel }}</div>
          </div>
          <button
            class="account-btn secondary"
            type="button"
            :disabled="!pack.available || ordering === pack.id"
            @click="openPaymentDialog(pack.id)"
          >
            {{ pack.available ? '立即购买' : '敬请期待' }}
          </button>
        </article>
      </div>
    </section>

    <!-- 支付方式选择对话框 -->
    <div v-if="showPayDialog" class="pay-dialog-mask" @click.self="showPayDialog = false">
      <div class="pay-dialog">
        <h3>选择支付方式</h3>
        <div class="pay-methods">
          <button
            class="pay-method-btn"
            :class="{ active: payMethod === 'wechat' }"
            type="button"
            @click="payMethod = 'wechat'"
          >
            <span class="material-icons-round" style="color: #07c160">smartphone</span>
            微信支付
          </button>
          <button
            class="pay-method-btn"
            :class="{ active: payMethod === 'alipay' }"
            type="button"
            @click="payMethod = 'alipay'"
          >
            <span class="material-icons-round" style="color: #1677ff">account_balance_wallet</span>
            支付宝
          </button>
        </div>

        <!-- 微信二维码 -->
        <div v-if="codeUrl && payMethod === 'wechat'" class="pay-qr-wrap">
          <p style="margin-bottom: 8px; color: var(--text-secondary); font-size: 13px">请使用微信扫码支付</p>
          <div class="pay-qr-placeholder">
            <span class="material-icons-round" style="font-size:48px; color: #07c160">qr_code_2</span>
            <small style="display:block; font-size:11px; color: var(--text-secondary); margin-top:4px">{{ codeUrl }}</small>
          </div>
          <p v-if="pollStatus === 'paid'" style="color: #22c55e; font-weight: 600">✓ 支付成功！</p>
          <p v-else style="color: var(--text-secondary); font-size:13px">等待支付...</p>
        </div>

        <!-- 支付宝跳转 -->
        <div v-if="payUrl && payMethod === 'alipay'" class="pay-qr-wrap">
          <a :href="payUrl" target="_blank" class="account-btn primary" style="display: inline-flex">
            <span class="material-icons-round">open_in_new</span>
            跳转支付宝付款
          </a>
        </div>

        <div class="pay-dialog-actions">
          <button class="account-btn primary" type="button" :disabled="!!codeUrl || !!payUrl || ordering !== ''" @click="createOrder">
            {{ codeUrl || payUrl ? '等待支付中' : '确认下单' }}
          </button>
          <button class="account-btn ghost" type="button" @click="showPayDialog = false">取消</button>
        </div>
      </div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onUnmounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'

const membershipPlans = [
  {
    id: 'monthly', name: '月度会员', priceLabel: '¥29', priceUnit: '/ 月',
    featured: false, features: ['文件夹投喂', '本地脚本运行', '无限次对话'],
  },
  {
    id: 'yearly', name: '年度会员', priceLabel: '¥299', priceUnit: '/ 年',
    originalPrice: '¥348/年', badge: '推荐 · 8 折优惠', featured: true,
    features: ['包含所有月度权益', '优先体验新功能', '专属客服支持'],
  },
]

const tokenPacks = [
  { id: 'pack-100w', name: '100万 Token 加油包', desc: '不限时间，用完为止', priceLabel: '¥9.9', available: true },
  { id: 'pack-500w', name: '500万 Token 加油包', desc: '敬请期待', priceLabel: '--', available: false },
]

const ordering = ref('')
const showPayDialog = ref(false)
const payMethod = ref<'wechat' | 'alipay'>('wechat')
const selectedPlanId = ref('')
const codeUrl = ref('')
const payUrl = ref('')
const outTradeNo = ref('')
const pollStatus = ref('')
let pollTimer: ReturnType<typeof setInterval> | null = null

function openPaymentDialog(planId: string) {
  selectedPlanId.value = planId
  codeUrl.value = ''
  payUrl.value = ''
  outTradeNo.value = ''
  pollStatus.value = ''
  showPayDialog.value = true
}

async function createOrder() {
  ordering.value = selectedPlanId.value
  try {
    const result = await invoke<{ out_trade_no: string; code_url?: string; pay_url?: string }>(
      'petool_create_order',
      { planId: selectedPlanId.value, paymentMethod: payMethod.value }
    )
    outTradeNo.value = result.out_trade_no
    codeUrl.value = result.code_url ?? ''
    payUrl.value = result.pay_url ?? ''

    // 开始轮询订单状态
    if (outTradeNo.value) {
      pollTimer = setInterval(async () => {
        try {
          const status = await invoke<{ status: string }>('petool_query_order', { outTradeNo: outTradeNo.value })
          if (status.status === 'paid') {
            pollStatus.value = 'paid'
            stopPoll()
            setTimeout(() => { showPayDialog.value = false }, 2000)
          }
        } catch { /* ignore */ }
      }, 3000)
    }
  } catch (e: any) {
    alert(e?.toString() || '下单失败')
  } finally {
    ordering.value = ''
  }
}

function stopPoll() {
  if (pollTimer) { clearInterval(pollTimer); pollTimer = null }
}

onUnmounted(stopPoll)
</script>

<style scoped>
.pay-dialog-mask {
  position: fixed; inset: 0; background: rgba(0,0,0,0.5);
  display: flex; align-items: center; justify-content: center; z-index: 999;
}
.pay-dialog {
  background: var(--bg-secondary, #1e1e2e);
  border: 1px solid var(--border, rgba(255,255,255,0.1));
  border-radius: 16px; padding: 28px; min-width: 340px;
  display: flex; flex-direction: column; gap: 16px;
}
.pay-dialog h3 { margin: 0; font-size: 16px; }
.pay-methods { display: flex; gap: 12px; }
.pay-method-btn {
  flex: 1; padding: 10px; border-radius: 10px;
  border: 2px solid var(--border, rgba(255,255,255,0.1));
  background: transparent; cursor: pointer; color: inherit;
  display: flex; align-items: center; justify-content: center; gap: 8px;
  font-size: 14px; transition: border-color 0.2s;
}
.pay-method-btn.active { border-color: var(--accent, #8b5cf6); }
.pay-qr-wrap { text-align: center; padding: 12px 0; }
.pay-qr-placeholder { display: inline-flex; flex-direction: column; align-items: center; }
.pay-dialog-actions { display: flex; gap: 8px; }
</style>
