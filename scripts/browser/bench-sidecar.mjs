import fs from 'node:fs/promises'
import path from 'node:path'
import process from 'node:process'
import readline from 'node:readline'
import { spawn } from 'node:child_process'

const repoRoot = process.cwd()
const sidecarEntry = process.env.BENCH_SIDECAR_ENTRY
  || path.join(repoRoot, 'browser-sidecar', 'src', 'index.mjs')
const iterations = Math.max(1, Number.parseInt(process.env.BENCH_ITERATIONS || '20', 10) || 20)
const profileName = process.env.BENCH_PROFILE || 'openclaw'
const outputPath = path.join(repoRoot, 'tmp', 'bench', 'browser-latency.json')
const appLogDir = path.join(repoRoot, 'tmp', 'bench', 'logs')
const profilesRoot = path.join(repoRoot, 'tmp', 'bench', 'profiles')

function percentile(values, p) {
  if (values.length === 0) return 0
  const sorted = [...values].sort((a, b) => a - b)
  const index = Math.min(sorted.length - 1, Math.max(0, Math.ceil((p / 100) * sorted.length) - 1))
  return sorted[index]
}

function buildBenchDataUrl() {
  const html = `
<!doctype html>
<html>
<body style="font-family: sans-serif; padding: 24px;">
  <label for="q">Search</label>
  <input id="q" placeholder="type here" />
  <button id="submit" onclick="window.__bench_clicks = (window.__bench_clicks || 0) + 1;">Submit</button>
</body>
</html>`
  return `data:text/html,${encodeURIComponent(html)}`
}

class SidecarClient {
  constructor(entry) {
    this.entry = entry
    this.proc = null
    this.nextId = 1
    this.pending = new Map()
  }

  start() {
    this.proc = spawn(process.execPath, [this.entry], {
      stdio: ['pipe', 'pipe', 'pipe']
    })
    const rl = readline.createInterface({ input: this.proc.stdout, crlfDelay: Infinity })
    rl.on('line', (line) => {
      if (!line.trim()) return
      let parsed
      try {
        parsed = JSON.parse(line)
      } catch (error) {
        return
      }
      const ticket = this.pending.get(parsed.id)
      if (!ticket) return
      this.pending.delete(parsed.id)
      ticket.resolve(parsed)
    })
    this.proc.stderr.on('data', (chunk) => {
      process.stderr.write(String(chunk))
    })
  }

  async call(method, params, timeoutMs = 30_000) {
    if (!this.proc || !this.proc.stdin.writable) {
      throw new Error('Sidecar process is not running')
    }
    const id = this.nextId++
    const payload = JSON.stringify({ id, method, params })
    return await new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.pending.delete(id)
        reject(new Error(`Sidecar call timed out: method=${method}, timeout_ms=${timeoutMs}`))
      }, timeoutMs)
      this.pending.set(id, {
        resolve: (value) => {
          clearTimeout(timer)
          resolve(value)
        }
      })
      this.proc.stdin.write(`${payload}\n`)
    })
  }

  async close() {
    if (!this.proc) return
    try {
      await this.call('shutdown', {}, 10_000).catch(() => null)
    } finally {
      this.proc.kill()
      this.proc = null
    }
  }
}

async function main() {
  const cdpUrl = typeof process.env.BENCH_BROWSER_CDP_URL === 'string'
    ? process.env.BENCH_BROWSER_CDP_URL.trim()
    : ''
  const executablePath = typeof process.env.BENCH_BROWSER_EXECUTABLE_PATH === 'string'
    ? process.env.BENCH_BROWSER_EXECUTABLE_PATH.trim()
    : ''
  if (!cdpUrl && !executablePath) {
    throw new Error('Set BENCH_BROWSER_CDP_URL or BENCH_BROWSER_EXECUTABLE_PATH before running bench-sidecar.')
  }

  await fs.mkdir(appLogDir, { recursive: true })
  await fs.mkdir(path.dirname(outputPath), { recursive: true })

  const browserConfig = {
    enabled: true,
    default_profile: profileName,
    evaluate_enabled: false,
    allow_private_network: false,
    performance_preset: 'balanced',
    capture_response_bodies: false,
    default_act_timeout_ms: 1400,
    operation_timeout_ms: 20_000,
    profiles: {
      [profileName]: {
        engine: 'chrome',
        headless: process.env.BENCH_HEADLESS !== '0',
        executable_path: executablePath || null,
        cdp_url: cdpUrl || null,
        user_data_dir: null,
        color: '#FF6A00',
        viewport: { width: 1280, height: 800 }
      }
    }
  }
  const paths = {
    profiles_root: profilesRoot,
    app_log_dir: appLogDir
  }
  const benchUrl = buildBenchDataUrl()
  const navigateUrl = buildBenchDataUrl()

  const client = new SidecarClient(sidecarEntry)
  client.start()

  const stats = {
    open: [],
    snapshot: [],
    act_type: [],
    act_click: [],
    navigate_light: [],
    navigate_full: [],
    act_separate_3step: [],
    act_batch_3step: []
  }

  async function browserAction(action, params = {}, targetId = null) {
    const request = {
      action,
      profile: profileName,
      target_id: targetId,
      params
    }
    const response = await client.call('browser.action', {
      request,
      browser_config: browserConfig,
      paths
    })
    if (!response.ok) {
      throw new Error(response.error || `Action failed: ${action}`)
    }
    return response
  }

  try {
    await browserAction('start')

    for (let i = 0; i < iterations; i += 1) {
      const openRes = await browserAction('open', { url: benchUrl })
      const targetId = openRes.data.target_id
      stats.open.push(Number(openRes.meta?.duration_ms) || 0)

      const snapshotRes = await browserAction('snapshot', { mode: 'compact', max_refs: 80 }, targetId)
      stats.snapshot.push(Number(snapshotRes.meta?.duration_ms) || 0)

      const typeRes = await browserAction('act', {
        kind: 'type',
        selector: '#q',
        text: `hello-${i}`,
        strategy: 'balanced'
      }, targetId)
      stats.act_type.push(Number(typeRes.meta?.duration_ms) || 0)

      const clickRes = await browserAction('act', {
        kind: 'click',
        selector: '#submit',
        strategy: 'balanced'
      }, targetId)
      stats.act_click.push(Number(clickRes.meta?.duration_ms) || 0)

      const navRes = await browserAction('navigate', {
        url: navigateUrl,
        include_links: false
      }, targetId)
      stats.navigate_light.push(Number(navRes.meta?.duration_ms) || 0)

      const navFullRes = await browserAction('navigate', {
        url: navigateUrl,
        include_links: true
      }, targetId)
      stats.navigate_full.push(Number(navFullRes.meta?.duration_ms) || 0)

      await browserAction('navigate', { url: benchUrl, include_links: false }, targetId)
      const separateStart = Date.now()
      const separate1 = await browserAction('act', {
        kind: 'type',
        selector: '#q',
        text: `separate-${i}`,
        strategy: 'balanced'
      }, targetId)
      const separate2 = await browserAction('act', {
        kind: 'click',
        selector: '#submit',
        strategy: 'balanced'
      }, targetId)
      const separate3 = await browserAction('act', {
        kind: 'wait',
        timeout_ms: 50,
        strategy: 'balanced'
      }, targetId)
      const separateMetaTotal =
        (Number(separate1.meta?.duration_ms) || 0) +
        (Number(separate2.meta?.duration_ms) || 0) +
        (Number(separate3.meta?.duration_ms) || 0)
      const separateWall = Date.now() - separateStart
      stats.act_separate_3step.push(Math.max(separateMetaTotal, separateWall))

      await browserAction('navigate', { url: benchUrl, include_links: false }, targetId)
      const batchStart = Date.now()
      const batchRes = await browserAction('act_batch', {
        actions: [
          { kind: 'type', selector: '#q', text: `batch-${i}`, strategy: 'balanced' },
          { kind: 'click', selector: '#submit', strategy: 'balanced' },
          { kind: 'wait', timeout_ms: 50, strategy: 'balanced' }
        ],
        stop_on_error: true
      }, targetId)
      const batchMetaTotal = Number(batchRes.meta?.duration_ms) || 0
      const batchWall = Date.now() - batchStart
      stats.act_batch_3step.push(Math.max(batchMetaTotal, batchWall))
    }

    const summary = Object.fromEntries(
      Object.entries(stats).map(([name, values]) => [
        name,
        {
          count: values.length,
          p50_ms: percentile(values, 50),
          p95_ms: percentile(values, 95),
          avg_ms: values.length > 0 ? Number((values.reduce((a, b) => a + b, 0) / values.length).toFixed(2)) : 0
        }
      ])
    )

    const avgSeparate = summary.act_separate_3step?.avg_ms || 0
    const avgBatch = summary.act_batch_3step?.avg_ms || 0
    const avgNavFull = summary.navigate_full?.avg_ms || 0
    const avgNavLight = summary.navigate_light?.avg_ms || 0
    const comparisons = {
      act_batch_vs_separate_reduction_pct: avgSeparate > 0
        ? Number((((avgSeparate - avgBatch) / avgSeparate) * 100).toFixed(2))
        : 0,
      navigate_light_vs_full_reduction_pct: avgNavFull > 0
        ? Number((((avgNavFull - avgNavLight) / avgNavFull) * 100).toFixed(2))
        : 0
    }

    const report = {
      generated_at: new Date().toISOString(),
      sidecar_entry: sidecarEntry,
      profile: profileName,
      iterations,
      summary,
      comparisons,
      raw: stats
    }
    await fs.writeFile(outputPath, JSON.stringify(report, null, 2), 'utf8')
    process.stdout.write(`[bench-sidecar] wrote ${outputPath}\n`)
    process.stdout.write(`${JSON.stringify({ summary, comparisons }, null, 2)}\n`)
  } finally {
    await browserAction('stop', {}, null).catch(() => null)
    await client.close()
  }
}

main().catch((error) => {
  process.stderr.write(`[bench-sidecar] failed: ${error instanceof Error ? error.message : String(error)}\n`)
  process.exit(1)
})
