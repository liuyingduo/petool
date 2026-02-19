<template>
  <div class="automation-panel">
    <el-form :model="localAutomation" label-width="180px">
      <el-form-item label="Enable Automation">
        <el-switch v-model="localAutomation.enabled" />
      </el-form-item>
      <el-form-item label="Max Concurrent Runs">
        <el-input-number
          v-model="localAutomation.max_concurrent_runs"
          :min="1"
          :max="8"
          :step="1"
          style="width: 220px"
        />
      </el-form-item>
      <el-form-item label="Close Behavior">
        <el-select v-model="localAutomation.close_behavior" style="width: 260px">
          <el-option label="Ask Every Time" value="ask" />
          <el-option label="Minimize To Tray" value="minimize_to_tray" />
          <el-option label="Exit App" value="exit" />
        </el-select>
      </el-form-item>
    </el-form>

    <el-divider content-position="left">Heartbeat</el-divider>
    <el-form :model="localAutomation.heartbeat" label-width="180px">
      <el-form-item label="Enable Heartbeat">
        <el-switch v-model="localAutomation.heartbeat.enabled" />
      </el-form-item>
      <el-form-item label="Every Minutes">
        <el-input-number
          v-model="localAutomation.heartbeat.every_minutes"
          :min="1"
          :max="1440"
          :step="1"
          style="width: 220px"
        />
      </el-form-item>
      <el-form-item label="Target Conversation">
        <el-select
          v-model="localAutomation.heartbeat.target_conversation_id"
          clearable
          filterable
          style="width: 100%"
          placeholder="Leave empty to auto-create Heartbeat conversation"
        >
          <el-option
            v-for="conversation in conversations"
            :key="conversation.id"
            :label="conversation.title"
            :value="conversation.id"
          />
        </el-select>
      </el-form-item>
      <el-form-item label="Prompt">
        <el-input
          v-model="localAutomation.heartbeat.prompt"
          type="textarea"
          :rows="3"
        />
      </el-form-item>
      <el-form-item label="Model Override">
        <el-input
          v-model="localAutomation.heartbeat.model"
          placeholder="Optional"
        />
      </el-form-item>
      <el-form-item label="Workspace Directory">
        <el-input
          v-model="localAutomation.heartbeat.workspace_directory"
          placeholder="Optional"
        />
      </el-form-item>
      <el-form-item label="Tool Whitelist">
        <el-input
          v-model="heartbeatWhitelistText"
          type="textarea"
          :rows="5"
          placeholder="One tool per line, supports wildcard patterns."
        />
      </el-form-item>
      <el-form-item>
        <el-button @click="handleRunHeartbeatNow" :loading="runningHeartbeat">
          绔嬪嵆鎵ц Heartbeat
        </el-button>
      </el-form-item>
    </el-form>

    <el-divider content-position="left">Jobs</el-divider>
    <div class="job-toolbar">
      <el-button size="small" @click="refreshJobs">鍒锋柊</el-button>
      <el-button size="small" type="primary" @click="openCreateJobDialog">鏂板缓浠诲姟</el-button>
    </div>

    <el-table :data="schedulerStore.jobs" size="small" style="width: 100%" height="220">
      <el-table-column prop="name" label="鍚嶇О" min-width="140" />
      <el-table-column label="鍚敤" width="88">
        <template #default="{ row }">
          <el-switch
            :model-value="row.enabled"
            size="small"
            @change="(value: boolean) => handleToggleJob(row.id, value)"
          />
        </template>
      </el-table-column>
      <el-table-column label="璋冨害" min-width="168">
        <template #default="{ row }">{{ formatSchedule(row) }}</template>
      </el-table-column>
      <el-table-column label="涓嬫杩愯" min-width="170">
        <template #default="{ row }">{{ formatTime(row.nextRunAt) }}</template>
      </el-table-column>
      <el-table-column label="最近状态" min-width="120">
        <template #default="{ row }">
          <span :class="['status-chip', row.lastStatus || 'none']">{{ row.lastStatus || 'n/a' }}</span>
        </template>
      </el-table-column>
      <el-table-column label="鎿嶄綔" width="232" fixed="right">
        <template #default="{ row }">
          <div class="row-actions">
            <el-button size="small" text @click="openEditJobDialog(row)">缂栬緫</el-button>
            <el-button size="small" text @click="handleRunJob(row.id)">鎵ц</el-button>
            <el-button size="small" text type="danger" @click="handleDeleteJob(row.id, row.name)">鍒犻櫎</el-button>
          </div>
        </template>
      </el-table-column>
    </el-table>

    <el-divider content-position="left">Runs</el-divider>
    <div class="run-toolbar">
      <el-select
        v-model="runFilterJobId"
        clearable
        placeholder="鍏ㄩ儴浠诲姟"
        style="width: 240px"
      >
        <el-option
          v-for="job in schedulerStore.jobs"
          :key="job.id"
          :label="job.name"
          :value="job.id"
        />
      </el-select>
      <el-button size="small" @click="refreshRuns">鍒锋柊</el-button>
    </div>

    <el-table :data="schedulerStore.runs" size="small" style="width: 100%" height="220">
      <el-table-column prop="jobNameSnapshot" label="浠诲姟" min-width="130" />
      <el-table-column prop="source" label="鏉ユ簮" width="90" />
      <el-table-column label="状态" width="90">
        <template #default="{ row }">
          <span :class="['status-chip', row.status]">{{ row.status }}</span>
        </template>
      </el-table-column>
      <el-table-column label="鏃堕棿" min-width="160">
        <template #default="{ row }">{{ formatTime(row.createdAt) }}</template>
      </el-table-column>
      <el-table-column label="鎽樿" min-width="220">
        <template #default="{ row }">{{ row.summary || row.error || '-' }}</template>
      </el-table-column>
      <el-table-column label="鎿嶄綔" width="86" fixed="right">
        <template #default="{ row }">
          <el-button size="small" text @click="openRunDetail(row)">璇︽儏</el-button>
        </template>
      </el-table-column>
    </el-table>

    <el-dialog v-model="jobDialogVisible" :title="jobDialogTitle" width="680px">
      <el-form :model="jobForm" label-width="170px">
        <el-form-item label="浠诲姟鍚嶇О">
          <el-input v-model="jobForm.name" />
        </el-form-item>
        <el-form-item label="鎻忚堪">
          <el-input v-model="jobForm.description" />
        </el-form-item>
        <el-form-item label="鍚敤">
          <el-switch v-model="jobForm.enabled" />
        </el-form-item>
        <el-form-item label="璋冨害绫诲瀷">
          <el-radio-group v-model="jobForm.scheduleKind">
            <el-radio value="at">at</el-radio>
            <el-radio value="every">every</el-radio>
            <el-radio value="cron">cron</el-radio>
          </el-radio-group>
        </el-form-item>
        <el-form-item label="At 鏃堕棿" v-if="jobForm.scheduleKind === 'at'">
          <el-input v-model="jobForm.scheduleAt" placeholder="2026-02-20T08:30:00+08:00" />
        </el-form-item>
        <el-form-item label="Every 绉掓暟" v-if="jobForm.scheduleKind === 'every'">
          <el-input-number v-model="jobForm.everySeconds" :min="1" :max="86400" />
        </el-form-item>
        <el-form-item label="Cron 表达式" v-if="jobForm.scheduleKind === 'cron'">
          <el-input v-model="jobForm.cronExpr" placeholder="0 */30 * * * * *" />
        </el-form-item>
        <el-form-item label="Cron 鏃跺尯" v-if="jobForm.scheduleKind === 'cron'">
          <el-input v-model="jobForm.timezone" placeholder="Asia/Shanghai (optional)" />
        </el-form-item>
        <el-form-item label="Session Mode">
          <el-select v-model="jobForm.sessionTarget" style="width: 220px">
            <el-option label="main" value="main" />
            <el-option label="isolated" value="isolated" />
          </el-select>
        </el-form-item>
        <el-form-item label="鐩爣浼氳瘽">
          <el-select v-model="jobForm.targetConversationId" filterable style="width: 100%">
            <el-option
              v-for="conversation in conversations"
              :key="conversation.id"
              :label="conversation.title"
              :value="conversation.id"
            />
          </el-select>
        </el-form-item>
        <el-form-item label="娑堟伅妯℃澘">
          <el-input v-model="jobForm.message" type="textarea" :rows="3" />
        </el-form-item>
        <el-form-item label="妯″瀷瑕嗙洊">
          <el-input v-model="jobForm.modelOverride" placeholder="Optional" />
        </el-form-item>
        <el-form-item label="工作区目录">
          <el-input v-model="jobForm.workspaceDirectory" placeholder="Optional" />
        </el-form-item>
        <el-form-item label="超时（秒）">
          <el-input-number v-model="jobForm.runTimeoutSeconds" :min="30" :max="86400" />
        </el-form-item>
        <el-form-item label="完成后删除（at）">
          <el-switch v-model="jobForm.deleteAfterRun" />
        </el-form-item>
        <el-form-item label="工具白名单">
          <el-input
            v-model="jobWhitelistText"
            type="textarea"
            :rows="5"
            placeholder="One tool per line"
          />
        </el-form-item>
      </el-form>
      <template #footer>
        <el-button @click="jobDialogVisible = false">鍙栨秷</el-button>
        <el-button type="primary" @click="saveJob" :loading="savingJob">淇濆瓨</el-button>
      </template>
    </el-dialog>

    <el-dialog v-model="runDetailVisible" title="Run Detail" width="760px">
      <div v-if="selectedRun">
        <div class="run-meta">
          <div>鐘舵€? {{ selectedRun.status }}</div>
          <div>鏉ユ簮: {{ selectedRun.source }}</div>
          <div>鏃堕棿: {{ formatTime(selectedRun.createdAt) }}</div>
        </div>
        <div v-if="selectedRun.summary" class="run-summary">{{ selectedRun.summary }}</div>
        <pre class="run-detail-json">{{ runDetailText }}</pre>
      </div>
    </el-dialog>
  </div>
</template>

<script setup lang="ts">
import { computed, onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { ElMessage, ElMessageBox } from 'element-plus'
import { type AutomationConfig } from '@/stores/config'
import {
  useSchedulerStore,
  type SchedulerJob,
  type SchedulerJobCreateInput,
  type SchedulerJobPatchInput,
  type SchedulerRun
} from '@/stores/scheduler'

interface Props {
  automation: AutomationConfig
}

interface Emits {
  (event: 'update:automation', value: AutomationConfig): void
}

interface ConversationOption {
  id: string
  title: string
}

interface JobFormState {
  id?: string
  name: string
  description: string
  enabled: boolean
  scheduleKind: 'at' | 'every' | 'cron'
  scheduleAt: string
  everySeconds: number
  cronExpr: string
  timezone: string
  sessionTarget: 'main' | 'isolated'
  targetConversationId: string
  message: string
  modelOverride: string
  workspaceDirectory: string
  runTimeoutSeconds: number
  deleteAfterRun: boolean
}

function deepClone<T>(value: T): T {
  return JSON.parse(JSON.stringify(value)) as T
}

function sanitizeWhitelist(text: string) {
  return text
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter((line, index, list) => line.length > 0 && list.indexOf(line) === index)
}

function normalizeNullable(value: string) {
  const trimmed = value.trim()
  return trimmed.length > 0 ? trimmed : null
}

const props = defineProps<Props>()
const emit = defineEmits<Emits>()
const schedulerStore = useSchedulerStore()

const localAutomation = ref<AutomationConfig>(deepClone(props.automation))
const conversations = ref<ConversationOption[]>([])
const runningHeartbeat = ref(false)
const runFilterJobId = ref<string | undefined>(undefined)

const jobDialogVisible = ref(false)
const savingJob = ref(false)
const jobForm = ref<JobFormState>({
  name: '',
  description: '',
  enabled: true,
  scheduleKind: 'every',
  scheduleAt: '',
  everySeconds: 1800,
  cronExpr: '',
  timezone: '',
  sessionTarget: 'main',
  targetConversationId: '',
  message: '',
  modelOverride: '',
  workspaceDirectory: '',
  runTimeoutSeconds: 600,
  deleteAfterRun: false
})
const jobWhitelistText = ref('')

const runDetailVisible = ref(false)
const selectedRun = ref<SchedulerRun | null>(null)

const jobDialogTitle = computed(() => (jobForm.value.id ? '缂栬緫浠诲姟' : '鏂板缓浠诲姟'))

const heartbeatWhitelistText = computed({
  get: () => (localAutomation.value.heartbeat.tool_whitelist || []).join('\n'),
  set: (value: string) => {
    localAutomation.value.heartbeat.tool_whitelist = sanitizeWhitelist(value)
  }
})

const runDetailText = computed(() => {
  if (!selectedRun.value) return ''
  try {
    return JSON.stringify(selectedRun.value.detailJson, null, 2)
  } catch {
    return String(selectedRun.value.detailJson)
  }
})

watch(
  () => props.automation,
  (value) => {
    localAutomation.value = deepClone(value)
  },
  { immediate: true, deep: true }
)

watch(
  localAutomation,
  (value) => {
    emit('update:automation', deepClone(value))
  },
  { deep: true }
)

function formatSchedule(job: SchedulerJob) {
  if (job.scheduleKind === 'at') {
    return `at ${job.scheduleAt || '-'}`
  }
  if (job.scheduleKind === 'every') {
    const seconds = Math.max(1, Math.floor((job.everyMs || 0) / 1000))
    return `every ${seconds}s`
  }
  return `cron ${job.cronExpr || '-'}${job.timezone ? ` (${job.timezone})` : ''}`
}

function formatTime(value?: string | null) {
  if (!value) return '-'
  const date = new Date(value)
  if (Number.isNaN(date.getTime())) return value
  return date.toLocaleString('zh-CN', { hour12: false })
}

async function loadConversations() {
  try {
    const rows = await invoke<Array<{ id: string; title: string }>>('get_conversations')
    conversations.value = rows.map((item) => ({ id: item.id, title: item.title || item.id }))
  } catch {
    conversations.value = []
  }
}

async function refreshJobs() {
  await schedulerStore.loadJobs(true)
}

async function refreshRuns() {
  await schedulerStore.loadRuns(runFilterJobId.value, 100)
}

async function handleRunHeartbeatNow() {
  runningHeartbeat.value = true
  try {
    const result = await schedulerStore.runHeartbeatNow()
    if (!result.accepted) {
      ElMessage.warning(result.reason || 'Heartbeat 未执行')
      return
    }
    ElMessage.success('Heartbeat 已触发')
    await refreshRuns()
  } catch (error) {
    ElMessage.error(String(error))
  } finally {
    runningHeartbeat.value = false
  }
}

function openCreateJobDialog() {
  const conversationId =
    localAutomation.value.heartbeat.target_conversation_id ||
    conversations.value[0]?.id ||
    ''
  jobForm.value = {
    name: '',
    description: '',
    enabled: true,
    scheduleKind: 'every',
    scheduleAt: '',
    everySeconds: 1800,
    cronExpr: '',
    timezone: '',
    sessionTarget: 'main',
    targetConversationId: conversationId,
    message: '',
    modelOverride: '',
    workspaceDirectory: '',
    runTimeoutSeconds: 600,
    deleteAfterRun: false
  }
  jobWhitelistText.value = (localAutomation.value.heartbeat.tool_whitelist || []).join('\n')
  jobDialogVisible.value = true
}

function openEditJobDialog(job: SchedulerJob) {
  jobForm.value = {
    id: job.id,
    name: job.name,
    description: job.description || '',
    enabled: job.enabled,
    scheduleKind: job.scheduleKind === 'cron' ? 'cron' : job.scheduleKind === 'at' ? 'at' : 'every',
    scheduleAt: job.scheduleAt || '',
    everySeconds: Math.max(1, Math.floor((job.everyMs || 0) / 1000)),
    cronExpr: job.cronExpr || '',
    timezone: job.timezone || '',
    sessionTarget: job.sessionTarget === 'isolated' ? 'isolated' : 'main',
    targetConversationId: job.targetConversationId,
    message: job.message,
    modelOverride: job.modelOverride || '',
    workspaceDirectory: job.workspaceDirectory || '',
    runTimeoutSeconds: job.runTimeoutSeconds || 600,
    deleteAfterRun: job.deleteAfterRun
  }
  jobWhitelistText.value = (job.toolWhitelist || []).join('\n')
  jobDialogVisible.value = true
}

async function saveJob() {
  const whitelist = sanitizeWhitelist(jobWhitelistText.value)
  if (!jobForm.value.name.trim()) {
    ElMessage.warning('请输入任务名称')
    return
  }
  if (!jobForm.value.targetConversationId.trim()) {
    ElMessage.warning('请选择目标会话')
    return
  }
  if (!jobForm.value.message.trim()) {
    ElMessage.warning('请输入消息模板')
    return
  }
  if (jobForm.value.scheduleKind === 'at' && !jobForm.value.scheduleAt.trim()) {
    ElMessage.warning('请填写 at 时间')
    return
  }
  if (jobForm.value.scheduleKind === 'cron' && !jobForm.value.cronExpr.trim()) {
    ElMessage.warning('请填写 cron 表达式')
    return
  }

  savingJob.value = true
  try {
    if (jobForm.value.id) {
      const patch: SchedulerJobPatchInput = {
        name: jobForm.value.name.trim(),
        description: normalizeNullable(jobForm.value.description),
        enabled: jobForm.value.enabled,
        scheduleKind: jobForm.value.scheduleKind,
        scheduleAt: jobForm.value.scheduleKind === 'at' ? normalizeNullable(jobForm.value.scheduleAt) : null,
        everyMs: jobForm.value.scheduleKind === 'every' ? Math.max(1000, Math.trunc(jobForm.value.everySeconds * 1000)) : null,
        cronExpr: jobForm.value.scheduleKind === 'cron' ? normalizeNullable(jobForm.value.cronExpr) : null,
        timezone: jobForm.value.scheduleKind === 'cron' ? normalizeNullable(jobForm.value.timezone) : null,
        sessionTarget: jobForm.value.sessionTarget,
        targetConversationId: jobForm.value.targetConversationId.trim(),
        message: jobForm.value.message.trim(),
        modelOverride: normalizeNullable(jobForm.value.modelOverride),
        workspaceDirectory: normalizeNullable(jobForm.value.workspaceDirectory),
        runTimeoutSeconds: Math.max(30, Math.min(86400, Math.trunc(jobForm.value.runTimeoutSeconds))),
        deleteAfterRun: jobForm.value.deleteAfterRun,
        toolWhitelist: whitelist
      }
      await schedulerStore.updateJob(jobForm.value.id, patch)
      ElMessage.success('任务已更新')
    } else {
      const input: SchedulerJobCreateInput = {
        name: jobForm.value.name.trim(),
        description: normalizeNullable(jobForm.value.description),
        enabled: jobForm.value.enabled,
        scheduleKind: jobForm.value.scheduleKind,
        scheduleAt: jobForm.value.scheduleKind === 'at' ? normalizeNullable(jobForm.value.scheduleAt) : null,
        everyMs: jobForm.value.scheduleKind === 'every' ? Math.max(1000, Math.trunc(jobForm.value.everySeconds * 1000)) : null,
        cronExpr: jobForm.value.scheduleKind === 'cron' ? normalizeNullable(jobForm.value.cronExpr) : null,
        timezone: jobForm.value.scheduleKind === 'cron' ? normalizeNullable(jobForm.value.timezone) : null,
        sessionTarget: jobForm.value.sessionTarget,
        targetConversationId: jobForm.value.targetConversationId.trim(),
        message: jobForm.value.message.trim(),
        modelOverride: normalizeNullable(jobForm.value.modelOverride),
        workspaceDirectory: normalizeNullable(jobForm.value.workspaceDirectory),
        runTimeoutSeconds: Math.max(30, Math.min(86400, Math.trunc(jobForm.value.runTimeoutSeconds))),
        deleteAfterRun: jobForm.value.deleteAfterRun,
        toolWhitelist: whitelist
      }
      await schedulerStore.createJob(input)
      ElMessage.success('任务已创建')
    }

    jobDialogVisible.value = false
    await refreshJobs()
  } catch (error) {
    ElMessage.error(String(error))
  } finally {
    savingJob.value = false
  }
}

async function handleToggleJob(jobId: string, enabled: boolean) {
  try {
    await schedulerStore.updateJob(jobId, { enabled })
  } catch (error) {
    ElMessage.error(String(error))
    await refreshJobs()
  }
}

async function handleRunJob(jobId: string) {
  try {
    const result = await schedulerStore.runJobNow(jobId)
    if (!result.accepted) {
      ElMessage.warning(result.reason || '任务未执行')
      return
    }
    ElMessage.success('任务已触发')
    await refreshRuns()
  } catch (error) {
    ElMessage.error(String(error))
  }
}

async function handleDeleteJob(jobId: string, name: string) {
  try {
    await ElMessageBox.confirm('确认删除任务「' + name + '」吗？', '删除任务', {
      type: 'warning',
      confirmButtonText: '删除',
      cancelButtonText: '取消'
    })
    await schedulerStore.deleteJob(jobId)
    ElMessage.success('任务已删除')
  } catch (error) {
    if (error !== 'cancel' && error !== 'close') {
      ElMessage.error(String(error))
    }
  }
}

function openRunDetail(run: SchedulerRun) {
  selectedRun.value = run
  runDetailVisible.value = true
}

onMounted(async () => {
  await Promise.all([
    loadConversations(),
    schedulerStore.ensureListeners()
  ])
  await schedulerStore.refreshAll()
})

onBeforeUnmount(() => {
  schedulerStore.disposeListeners()
})
</script>

<style scoped>
.automation-panel {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.job-toolbar,
.run-toolbar {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 8px;
}

.row-actions {
  display: flex;
  align-items: center;
  gap: 2px;
}

.status-chip {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  min-width: 48px;
  padding: 2px 8px;
  border-radius: 999px;
  font-size: 12px;
  line-height: 1.4;
  background: #f3f4f6;
  color: #6b7280;
}

.status-chip.ok {
  background: #dcfce7;
  color: #166534;
}

.status-chip.error {
  background: #fee2e2;
  color: #b91c1c;
}

.status-chip.skipped {
  background: #fef3c7;
  color: #92400e;
}

.run-meta {
  display: flex;
  gap: 16px;
  margin-bottom: 8px;
  color: #4b5563;
  font-size: 13px;
}

.run-detail-json {
  margin: 0;
  max-height: 360px;
  overflow: auto;
  padding: 10px 12px;
  border-radius: 8px;
  background: #0f172a;
  color: #e2e8f0;
  font-family: Consolas, Monaco, monospace;
  font-size: 12px;
  line-height: 1.5;
}

.run-summary {
  margin-bottom: 8px;
  padding: 8px 10px;
  border-radius: 8px;
  background: #f8fafc;
  border: 1px solid #e2e8f0;
  color: #334155;
  font-size: 13px;
}
</style>

