import { defineStore } from 'pinia'
import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'

export type SchedulerScheduleKind = 'at' | 'every' | 'cron'
export type SchedulerSessionTarget = 'main' | 'isolated' | 'heartbeat'
export type SchedulerRunSource = 'job' | 'heartbeat'
export type SchedulerRunStatus = 'ok' | 'error' | 'skipped'

export interface SchedulerJob {
  id: string
  name: string
  description?: string | null
  enabled: boolean
  scheduleKind: SchedulerScheduleKind
  scheduleAt?: string | null
  everyMs?: number | null
  cronExpr?: string | null
  timezone?: string | null
  sessionTarget: SchedulerSessionTarget
  targetConversationId: string
  message: string
  modelOverride?: string | null
  workspaceDirectory?: string | null
  toolWhitelist: string[]
  runTimeoutSeconds: number
  deleteAfterRun: boolean
  nextRunAt?: string | null
  runningAt?: string | null
  lastRunAt?: string | null
  lastStatus?: SchedulerRunStatus | null
  lastError?: string | null
  lastDurationMs?: number | null
  consecutiveErrors: number
  createdAt: string
  updatedAt: string
}

export interface SchedulerRun {
  id: string
  source: SchedulerRunSource
  jobId?: string | null
  jobNameSnapshot: string
  targetConversationId: string
  sessionTarget: SchedulerSessionTarget
  triggeredAt: string
  startedAt: string
  endedAt: string
  status: SchedulerRunStatus
  error?: string | null
  summary?: string | null
  outputText?: string | null
  detailJson: Record<string, unknown>
  createdAt: string
}

export interface SchedulerStatus {
  enabled: boolean
  heartbeatEnabled: boolean
  runningJobs: number
  nextWakeAt?: string | null
}

export interface SchedulerRunRequest {
  accepted: boolean
  reason?: string | null
}

export interface SchedulerJobCreateInput {
  name: string
  description?: string | null
  enabled?: boolean
  scheduleKind: SchedulerScheduleKind
  scheduleAt?: string | null
  everyMs?: number | null
  cronExpr?: string | null
  timezone?: string | null
  sessionTarget: Exclude<SchedulerSessionTarget, 'heartbeat'>
  targetConversationId: string
  message: string
  modelOverride?: string | null
  workspaceDirectory?: string | null
  toolWhitelist?: string[]
  runTimeoutSeconds?: number
  deleteAfterRun?: boolean
}

export interface SchedulerJobPatchInput {
  name?: string
  description?: string | null
  enabled?: boolean
  scheduleKind?: SchedulerScheduleKind
  scheduleAt?: string | null
  everyMs?: number | null
  cronExpr?: string | null
  timezone?: string | null
  sessionTarget?: Exclude<SchedulerSessionTarget, 'heartbeat'>
  targetConversationId?: string
  message?: string
  modelOverride?: string | null
  workspaceDirectory?: string | null
  toolWhitelist?: string[]
  runTimeoutSeconds?: number
  deleteAfterRun?: boolean
}

interface SchedulerJobEvent {
  event: 'added' | 'updated' | 'removed'
  job?: SchedulerJob
  jobId?: string
}

export const useSchedulerStore = defineStore('scheduler', () => {
  const status = ref<SchedulerStatus | null>(null)
  const jobs = ref<SchedulerJob[]>([])
  const runs = ref<SchedulerRun[]>([])
  const loading = ref(false)
  const listenersReady = ref(false)
  const unlistenFns: Array<() => void> = []

  function upsertJob(job: SchedulerJob) {
    const index = jobs.value.findIndex((item) => item.id === job.id)
    if (index < 0) {
      jobs.value.unshift(job)
      return
    }
    jobs.value[index] = job
  }

  function removeJob(jobId: string) {
    jobs.value = jobs.value.filter((item) => item.id !== jobId)
  }

  function pushRun(run: SchedulerRun) {
    runs.value.unshift(run)
    if (runs.value.length > 400) {
      runs.value = runs.value.slice(0, 400)
    }
  }

  async function loadStatus() {
    status.value = await invoke<SchedulerStatus>('scheduler_get_status')
    return status.value
  }

  async function loadJobs(includeDisabled = true) {
    jobs.value = await invoke<SchedulerJob[]>('scheduler_list_jobs', {
      includeDisabled
    })
    return jobs.value
  }

  async function loadRuns(jobId?: string, limit = 100) {
    runs.value = await invoke<SchedulerRun[]>('scheduler_list_runs', {
      jobId: jobId || null,
      limit
    })
    return runs.value
  }

  async function refreshAll() {
    loading.value = true
    try {
      await Promise.all([loadStatus(), loadJobs(true), loadRuns(undefined, 100)])
    } finally {
      loading.value = false
    }
  }

  async function getJob(jobId: string) {
    return invoke<SchedulerJob | null>('scheduler_get_job', { jobId })
  }

  async function createJob(input: SchedulerJobCreateInput) {
    const job = await invoke<SchedulerJob>('scheduler_create_job', { input })
    upsertJob(job)
    return job
  }

  async function updateJob(jobId: string, patch: SchedulerJobPatchInput) {
    const job = await invoke<SchedulerJob>('scheduler_update_job', { jobId, patch })
    upsertJob(job)
    return job
  }

  async function deleteJob(jobId: string) {
    const removed = await invoke<boolean>('scheduler_delete_job', { jobId })
    if (removed) removeJob(jobId)
    return removed
  }

  async function runJobNow(jobId: string) {
    return invoke<SchedulerRunRequest>('scheduler_run_job_now', { jobId })
  }

  async function runHeartbeatNow() {
    return invoke<SchedulerRunRequest>('scheduler_run_heartbeat_now')
  }

  async function getRun(runId: string) {
    return invoke<SchedulerRun | null>('scheduler_get_run', { runId })
  }

  async function ensureListeners() {
    if (listenersReady.value) return
    listenersReady.value = true

    unlistenFns.push(
      await listen<SchedulerStatus>('scheduler-status', (event) => {
        if (!event.payload) return
        status.value = event.payload
      })
    )

    unlistenFns.push(
      await listen<SchedulerJobEvent>('scheduler-job-event', (event) => {
        const payload = event.payload
        if (!payload) return
        if (payload.event === 'removed' && payload.jobId) {
          removeJob(payload.jobId)
          return
        }
        if ((payload.event === 'added' || payload.event === 'updated') && payload.job) {
          upsertJob(payload.job)
        }
      })
    )

    unlistenFns.push(
      await listen<SchedulerRun>('scheduler-run-event', (event) => {
        if (!event.payload) return
        pushRun(event.payload)
      })
    )
  }

  function disposeListeners() {
    while (unlistenFns.length > 0) {
      const fn = unlistenFns.pop()
      if (fn) fn()
    }
    listenersReady.value = false
  }

  return {
    status,
    jobs,
    runs,
    loading,
    loadStatus,
    loadJobs,
    loadRuns,
    refreshAll,
    getJob,
    createJob,
    updateJob,
    deleteJob,
    runJobNow,
    runHeartbeatNow,
    getRun,
    ensureListeners,
    disposeListeners
  }
})
