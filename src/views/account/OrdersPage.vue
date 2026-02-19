<template>
  <div class="account-stack">
    <div class="account-two-col">
      <div class="account-card summary-card">
        <div class="summary-label">年度总消费</div>
        <div class="summary-value">{{ ordersSummary.yearlyAmount }} <small>2024年</small></div>
      </div>
      <div class="account-card summary-card">
        <div class="summary-label">近 30 天消费</div>
        <div class="summary-value">{{ ordersSummary.monthAmount }} <small>本月</small></div>
      </div>
    </div>

    <div class="orders-head">
      <h2>订单列表</h2>
      <div class="orders-toolbar">
        <button class="account-btn ghost small" type="button">
          <span class="material-icons-round">filter_list</span>
          筛选
        </button>
        <button class="account-btn ghost small" type="button">
          <span class="material-icons-round">download</span>
          导出
        </button>
      </div>
    </div>

    <div class="order-list">
      <article v-for="order in orderRecords" :key="order.id" class="account-card order-item" :class="{ refunded: order.status === 'refunded' }">
        <div class="order-main">
          <div class="order-id-row">
            <span class="order-id">{{ order.id }}</span>
            <button class="copy-btn" type="button" title="复制订单号">
              <span class="material-icons-round">content_copy</span>
            </button>
          </div>
          <h3>{{ order.title }}</h3>
          <div class="order-meta">
            <span><span class="material-icons-round">calendar_today</span>{{ order.createdAt }}</span>
            <span class="order-status" :class="order.status">
              {{ order.status === 'completed' ? '已完成' : '已退款' }}
            </span>
          </div>
        </div>
        <div class="order-side">
          <div class="order-amount">{{ order.amount }}</div>
          <button class="icon-btn" type="button" :title="order.status === 'completed' ? '查看发票' : '查看详情'">
            <span class="material-icons-round">{{ order.status === 'completed' ? 'receipt' : 'description' }}</span>
          </button>
        </div>
      </article>
    </div>

    <div class="center-row">
      <button class="text-link-btn" type="button">
        查看更多历史订单
        <span class="material-icons-round">expand_more</span>
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { orderRecords, ordersSummary } from './mock'
</script>
