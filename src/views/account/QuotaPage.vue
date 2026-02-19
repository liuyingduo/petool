<template>
  <div class="account-stack">
    <div v-if="loading" class="account-loading">
      <span class="material-icons-round spin">sync</span>
      加载中...
    </div>

    <template v-else-if="quota">
      <!-- 实时状态看板 -->
      <section class="account-section">
        <h2 class="section-title"><span class="material-icons-round">dashboard</span>实时状态看板</h2>
        <div class="quota-dashboard">
          <div class="account-card quota-total">
            <div class="summary-label">总余额 (Tokens)</div>
            <div class="quota-total-value">{{ quota.total_balance.toLocaleString() }}</div>
            <div class="quota-badge">今日已消耗: {{ quota.consumed_today.toLocaleString() }}</div>
          </div>

          <div class="account-card quota-trend">
            <div class="summary-label">近 7 天消费趋势</div>
            <div class="trend-bars">
              <div v-for="point in quota.trend" :key="point.date" class="trend-bar-col">
                <div class="trend-tooltip">{{ formatK(point.value) }}</div>
                <div
                  class="trend-bar"
                  :class="{ hot: point.value === peakValue }"
                  :style="{ height: `${barHeight(point.value)}%` }"
                ></div>
                <span class="trend-date" :class="{ hot: point.value === peakValue }">{{ point.date }}</span>
              </div>
            </div>
          </div>
        </div>
      </section>

      <!-- 消费明细 -->
      <section class="account-section">
        <div class="orders-head">
          <h2 class="section-title"><span class="material-icons-round">list_alt</span>消费明细清单</h2>
        </div>

        <div class="account-card usage-table-wrap">
          <div class="usage-head-row">
            <div>时间</div>
            <div>任务类型</div>
            <div>模型</div>
            <div class="right">消耗明细 (I/O)</div>
            <div class="right">小计</div>
          </div>
          <div class="usage-rows">
            <div
              v-for="row in usagePage.records"
              :key="row.id"
              class="usage-row"
            >
              <div class="cell mono">{{ row.created_at }}</div>
              <div class="cell">
                <div class="task-cell">
                  <span class="task-icon purple">
                    <span class="material-symbols-outlined">chat</span>
                  </span>
                  <span class="task-name">{{ row.task_type }}</span>
                </div>
              </div>
              <div class="cell">
                <span class="model-chip">{{ row.model }}</span>
              </div>
              <div class="cell right">
                <div class="io-detail">
                  <span>In: {{ row.prompt_tokens.toLocaleString() }}</span>
                  <span>Out: {{ row.completion_tokens.toLocaleString() }}</span>
                </div>
              </div>
              <div class="cell right amount-cell">
                <span>- {{ row.cost_tokens.toLocaleString() }}</span>
                <small>Tokens</small>
              </div>
            </div>
          </div>

          <!-- 分页 -->
          <div class="usage-pagination">
            <span>显示 {{ (currentPage - 1) * pageSize + 1 }}-{{ Math.min(currentPage * pageSize, usagePage.total) }} 共 {{ usagePage.total }} 条记录</span>
            <div class="pager">
              <button class="pager-btn" type="button" :disabled="currentPage <= 1" @click="goPage(currentPage - 1)">
                <span class="material-icons-round">chevron_left</span>
              </button>
              <button
                v-for="p in totalPages"
                :key="p"
                class="pager-btn"
                :class="{ active: p === currentPage }"
                type="button"
                @click="goPage(p)"
              >{{ p }}</button>
              <button class="pager-btn" type="button" :disabled="currentPage >= totalPages" @click="goPage(currentPage + 1)">
                <span class="material-icons-round">chevron_right</span>
              </button>
            </div>
          </div>
        </div>
      </section>
    </template>

    <div v-else class="account-card error-card">
      <span class="material-icons-round" style="color: var(--error)">error_outline</span>
      <p>{{ error || '加载失败，请先登录' }}</p>
      <button class="account-btn secondary" type="button" @click="loadData">重试</button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import { invoke } from '@tauri-apps/api/core'

interface QuotaDashboard {
  total_balance: number
  consumed_today: number
  trend: Array<{ date: string; value: number }>
}

interface UsagePage {
  records: Array<{
    id: string
    created_at: string
    task_type: string
    model: string
    prompt_tokens: number
    completion_tokens: number
    total_tokens: number
    cost_tokens: number
  }>
  total: number
  page: number
  page_size: number
}

const loading = ref(true)
const error = ref('')
const quota = ref<QuotaDashboard | null>(null)
const usagePage = ref<UsagePage>({ records: [], total: 0, page: 1, page_size: 10 })
const currentPage = ref(1)
const pageSize = 10

const peakValue = computed(() => Math.max(...(quota.value?.trend.map((t) => t.value) ?? [0])))
const totalPages = computed(() => Math.max(1, Math.ceil(usagePage.value.total / pageSize)))

function barHeight(value: number): number {
  const max = peakValue.value || 1
  return Math.max(18, Math.round((value / max) * 100))
}

function formatK(value: number): string {
  if (value >= 1000) return `${(value / 1000).toFixed(1)}k`
  return String(value)
}

async function loadData() {
  loading.value = true
  error.value = ''
  try {
    const [q, u] = await Promise.all([
      invoke<QuotaDashboard>('petool_get_quota'),
      invoke<UsagePage>('petool_get_usage', { page: currentPage.value, pageSize }),
    ])
    quota.value = q
    usagePage.value = u
  } catch (e: any) {
    error.value = e?.toString() || '未知错误'
  } finally {
    loading.value = false
  }
}

async function goPage(page: number) {
  currentPage.value = page
  try {
    usagePage.value = await invoke<UsagePage>('petool_get_usage', { page, pageSize })
  } catch {
    // ignore
  }
}

onMounted(() => { void loadData() })
</script>
