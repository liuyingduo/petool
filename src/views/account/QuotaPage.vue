<template>
  <div class="account-stack">
    <section class="account-section">
      <h2 class="section-title"><span class="material-icons-round">dashboard</span>实时状态看板</h2>
      <div class="quota-dashboard">
        <div class="account-card quota-total">
          <div class="summary-label">总余额 (Tokens)</div>
          <div class="quota-total-value">{{ quotaDashboard.totalBalance }}</div>
          <div class="quota-badge">今日已消耗: {{ quotaDashboard.consumedToday }}</div>
        </div>

        <div class="account-card quota-trend">
          <div class="summary-label">近 7 天消费趋势</div>
          <div class="trend-bars">
            <div v-for="point in quotaTrend" :key="point.date" class="trend-bar-col">
              <div class="trend-tooltip">{{ formatK(point.value) }}</div>
              <div class="trend-bar" :class="{ hot: point.value === peakValue }" :style="{ height: `${barHeight(point.value)}%` }"></div>
              <span class="trend-date" :class="{ hot: point.value === peakValue }">{{ point.date }}</span>
            </div>
          </div>
        </div>
      </div>
    </section>

    <section class="account-section">
      <div class="orders-head">
        <h2 class="section-title"><span class="material-icons-round">list_alt</span>消费明细清单</h2>
        <div class="orders-toolbar">
          <button class="account-btn ghost small" type="button">
            <span class="material-icons-round">tune</span>
            筛选
          </button>
          <button class="account-btn ghost small" type="button">
            <span class="material-icons-round">download</span>
            导出 CSV
          </button>
        </div>
      </div>

      <div class="account-card usage-table-wrap">
        <div class="usage-head-row">
          <div>时间</div>
          <div>任务类型</div>
          <div>模型档位</div>
          <div class="right">消耗明细 (I/O)</div>
          <div class="right">小计</div>
        </div>
        <div class="usage-rows">
          <div v-for="row in quotaUsageRecords" :key="row.id" class="usage-row" :class="{ emphasize: row.emphasize }">
            <div class="cell mono">{{ row.createdAt }}</div>
            <div class="cell">
              <div class="task-cell">
                <span class="task-icon" :class="row.taskIconClass">
                  <span class="material-symbols-outlined">{{ row.taskIcon }}</span>
                </span>
                <span class="task-name">{{ row.taskType }}</span>
              </div>
            </div>
            <div class="cell">
              <span class="model-chip">{{ row.model }}</span>
            </div>
            <div class="cell right">
              <div class="io-detail">
                <span>{{ row.detailA }}</span>
                <span>{{ row.detailB }}</span>
              </div>
            </div>
            <div class="cell right amount-cell">
              <span :class="{ hot: row.emphasize }">- {{ row.amount.toLocaleString() }}</span>
              <small>Tokens</small>
            </div>
          </div>
        </div>
        <div class="usage-pagination">
          <span>显示 1-5 共 128 条记录</span>
          <div class="pager">
            <button class="pager-btn" type="button"><span class="material-icons-round">chevron_left</span></button>
            <button class="pager-btn active" type="button">1</button>
            <button class="pager-btn" type="button">2</button>
            <button class="pager-btn" type="button">3</button>
            <button class="pager-btn" type="button"><span class="material-icons-round">chevron_right</span></button>
          </div>
        </div>
      </div>
    </section>
  </div>
</template>

<script setup lang="ts">
import { computed } from 'vue'
import { quotaDashboard, quotaTrend, quotaUsageRecords } from './mock'

const peakValue = computed(() => Math.max(...quotaTrend.map((item) => item.value)))

function barHeight(value: number): number {
  const max = peakValue.value || 1
  return Math.max(18, Math.round((value / max) * 100))
}

function formatK(value: number): string {
  return `${(value / 1000).toFixed(1)}k`
}
</script>
