<template>
  <div class="account-stack">
    <div v-if="loading" class="account-loading">
      <span class="material-icons-round spin">sync</span>
      加载中...
    </div>

    <template v-else>
      <section class="account-section">
        <div class="orders-head">
          <h2 class="section-title"><span class="material-icons-round">receipt_long</span>订单记录</h2>
        </div>

        <div class="account-card usage-table-wrap">
          <div class="usage-head-row" style="grid-template-columns: 1fr 2fr 1fr 1fr 1fr">
            <div>时间</div>
            <div>订单</div>
            <div>支付方式</div>
            <div class="right">金额</div>
            <div class="right">状态</div>
          </div>
          <div class="usage-rows">
            <div
              v-for="order in orders"
              :key="order.id"
              class="usage-row"
              style="grid-template-columns: 1fr 2fr 1fr 1fr 1fr"
            >
              <div class="cell mono">{{ order.created_at }}</div>
              <div class="cell">{{ order.title }}</div>
              <div class="cell">{{ order.payment_method === 'wechat' ? '微信支付' : '支付宝' }}</div>
              <div class="cell right">¥{{ order.amount.toFixed(2) }}</div>
              <div class="cell right">
                <span class="order-status" :class="order.status">
                  {{ statusLabel(order.status) }}
                </span>
              </div>
            </div>
            <div v-if="orders.length === 0" class="usage-row" style="justify-content: center; color: var(--text-secondary); grid-column: 1 / -1">
              暂无订单记录
            </div>
          </div>
        </div>
      </section>
    </template>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'

interface OrderRecord {
  id: string
  title: string
  amount: number
  plan_id: string
  payment_method: string
  status: string
  created_at: string
}

const loading = ref(true)
const orders = ref<OrderRecord[]>([])

function statusLabel(status: string): string {
  const map: Record<string, string> = {
    pending: '待支付',
    paid: '已完成',
    refunded: '已退款',
    cancelled: '已取消',
  }
  return map[status] ?? status
}

async function loadOrders() {
  loading.value = true
  try {
    orders.value = await invoke<OrderRecord[]>('petool_get_orders')
  } catch {
    // ignore
  } finally {
    loading.value = false
  }
}

onMounted(() => { void loadOrders() })
</script>

<style scoped>
.order-status {
  padding: 2px 8px;
  border-radius: 4px;
  font-size: 12px;
  font-weight: 500;
}
.order-status.paid { background: rgba(34, 197, 94, 0.15); color: #22c55e; }
.order-status.pending { background: rgba(251, 191, 36, 0.15); color: #f59e0b; }
.order-status.refunded { background: rgba(239, 68, 68, 0.15); color: #ef4444; }
.order-status.cancelled { background: rgba(148, 163, 184, 0.15); color: #94a3b8; }
</style>
