import fs from 'node:fs/promises'
import path from 'node:path'
import readline from 'node:readline'
import process from 'node:process'
import net from 'node:net'
import http from 'node:http'
import { spawn } from 'node:child_process'
import { chromium } from 'playwright'

const runtime = {
  profiles: new Map()
}

const SNAPSHOT_MARKER_ATTR = 'data-petool-ref'
const DEFAULT_SNAPSHOT_MAX_REFS = 120
const MAX_SNAPSHOT_MAX_REFS = 500
const DEFAULT_COMPACT_SNAPSHOT_REFS = 80
const PAGE_READY_LOADING_TIMEOUT_MS = 800
const PAGE_READY_NETWORK_IDLE_TIMEOUT_MS = 700
const PAGE_READY_NETWORK_IDLE_STABLE_MS = 120
const PAGE_READY_DOM_STABLE_MS = 180
const PAGE_READY_DOM_STABILITY_TIMEOUT_MS = 900
const TARGET_READY_TTL_MS = 15_000
const CDP_CONNECT_TIMEOUT_MS = 15_000
const CDP_CONNECT_RETRY_INTERVAL_MS = 180

function nowIso() {
  return new Date().toISOString()
}

function pushLimited(arr, item, limit = 200) {
  arr.push(item)
  if (arr.length > limit) {
    arr.splice(0, arr.length - limit)
  }
}

function makeErrorMessage(error) {
  if (!error) return 'Unknown error'
  if (typeof error === 'string') return error
  if (error instanceof Error) return error.message
  return String(error)
}

function isPrivateIpv4(host) {
  const parts = host.split('.').map((value) => Number.parseInt(value, 10))
  if (parts.length !== 4 || parts.some((v) => Number.isNaN(v) || v < 0 || v > 255)) return false
  if (parts[0] === 10) return true
  if (parts[0] === 127) return true
  if (parts[0] === 192 && parts[1] === 168) return true
  if (parts[0] === 172 && parts[1] >= 16 && parts[1] <= 31) return true
  if (parts[0] === 169 && parts[1] === 254) return true
  return false
}

function isPrivateHost(host) {
  const normalized = host.toLowerCase()
  if (normalized === 'localhost' || normalized.endsWith('.local')) return true
  if (normalized === '::1') return true
  return isPrivateIpv4(normalized)
}

function assertPrivateNetworkAllowed(targetUrl, allowPrivateNetwork) {
  if (allowPrivateNetwork) return
  let parsed
  try {
    parsed = new URL(targetUrl)
  } catch {
    return
  }
  if (!['http:', 'https:'].includes(parsed.protocol)) return
  if (isPrivateHost(parsed.hostname)) {
    throw new Error(
      `Blocked private network URL by policy: ${targetUrl}. Set browser.allow_private_network=true to allow.`
    )
  }
}

function requireObject(value, name) {
  if (!value || typeof value !== 'object' || Array.isArray(value)) {
    throw new Error(`${name} must be an object`)
  }
  return value
}

function resolveProfileName(request, browserConfig) {
  const requested = typeof request.profile === 'string' && request.profile.trim()
    ? request.profile.trim()
    : browserConfig.default_profile
  if (browserConfig.profiles?.[requested]) return requested
  if (browserConfig.profiles?.[browserConfig.default_profile]) return browserConfig.default_profile
  const first = Object.keys(browserConfig.profiles || {})[0]
  if (!first) throw new Error('No browser profile configured')
  return first
}

function profileUserDataDir(paths, profile) {
  return path.join(paths.profiles_root, profile, 'user-data')
}

function defaultViewport(profileConfig) {
  const viewport = profileConfig?.viewport || {}
  return {
    width: Number.isInteger(viewport.width) ? viewport.width : 1280,
    height: Number.isInteger(viewport.height) ? viewport.height : 800
  }
}

async function ensureDir(dirPath) {
  await fs.mkdir(dirPath, { recursive: true })
}

function sleep(ms) {
  return new Promise((resolve) => {
    setTimeout(resolve, ms)
  })
}

async function findFreeTcpPort() {
  return await new Promise((resolve, reject) => {
    const server = net.createServer()
    server.unref()
    server.on('error', reject)
    server.listen(0, '127.0.0.1', () => {
      const address = server.address()
      if (!address || typeof address === 'string') {
        server.close(() => {
          reject(new Error('Failed to allocate local port for remote debugging'))
        })
        return
      }
      const { port } = address
      server.close((error) => {
        if (error) {
          reject(error)
          return
        }
        resolve(port)
      })
    })
  })
}

async function fetchJson(url, timeoutMs = 1_200) {
  return await new Promise((resolve, reject) => {
    const request = http.get(url, { timeout: timeoutMs }, (response) => {
      const statusCode = Number(response.statusCode || 0)
      if (statusCode < 200 || statusCode >= 300) {
        response.resume()
        reject(new Error(`HTTP ${statusCode}`))
        return
      }

      const chunks = []
      response.on('data', (chunk) => chunks.push(chunk))
      response.on('error', reject)
      response.on('end', () => {
        try {
          const body = Buffer.concat(chunks).toString('utf8')
          resolve(JSON.parse(body))
        } catch (error) {
          reject(error)
        }
      })
    })

    request.on('timeout', () => {
      request.destroy(new Error('CDP probe timeout'))
    })
    request.on('error', reject)
  })
}

async function waitForCdpEndpoint(cdpUrl, timeoutMs = CDP_CONNECT_TIMEOUT_MS) {
  const versionEndpoint = `${cdpUrl}/json/version`
  const startedAt = Date.now()
  let lastError = 'unknown'
  while ((Date.now() - startedAt) < timeoutMs) {
    try {
      const payload = await fetchJson(versionEndpoint, 1_000)
      if (typeof payload?.webSocketDebuggerUrl === 'string' && payload.webSocketDebuggerUrl) {
        return {
          cdpUrl,
          webSocketDebuggerUrl: payload.webSocketDebuggerUrl
        }
      }
      lastError = 'CDP endpoint not ready'
    } catch (error) {
      lastError = makeErrorMessage(error)
    }
    await sleep(CDP_CONNECT_RETRY_INTERVAL_MS)
  }
  throw new Error(`Timed out waiting for CDP endpoint: ${cdpUrl} (${lastError})`)
}

async function terminateChildProcess(child) {
  if (!child || !Number.isInteger(child.pid)) return
  if (child.exitCode !== null || child.killed) return
  const pid = Number(child.pid)
  if (!Number.isInteger(pid) || pid <= 0) return

  if (process.platform === 'win32') {
    await new Promise((resolve) => {
      const killer = spawn('taskkill', ['/PID', String(pid), '/T', '/F'], {
        stdio: 'ignore',
        windowsHide: true
      })
      killer.on('error', () => resolve(undefined))
      killer.on('close', () => resolve(undefined))
    })
    return
  }

  try {
    process.kill(-pid, 'SIGTERM')
  } catch {
    try {
      process.kill(pid, 'SIGTERM')
    } catch {
      // ignore process termination errors
    }
  }
}

async function terminateOwnedBrowserProcess(state) {
  const child = state.launchedProcess
  state.launchedProcess = null
  state.launchedCdpUrl = null
  await terminateChildProcess(child)
}

function getProfileState(profile) {
  let state = runtime.profiles.get(profile)
  if (!state) {
    state = {
      browser: null,
      context: null,
      connectionMode: null,
      pages: new Map(),
      pageIds: new Map(),
      refsByTarget: new Map(),
      nextTargetSeq: 1,
      activeTargetId: null,
      consoleMessages: [],
      errors: [],
      requests: [],
      responseBodies: [],
      captureResponseBodies: false,
      headers: undefined,
      credentials: undefined,
      geolocation: undefined,
      media: undefined,
      timezone: undefined,
      locale: undefined,
      device: undefined,
      pendingReadyTargets: new Map(),
      inflightRequestsByTarget: new Map(),
      traceStarted: false,
      tracePath: null,
      launchedProcess: null,
      launchedCdpUrl: null
    }
    runtime.profiles.set(profile, state)
  }
  return state
}

function clearProfilePages(state) {
  state.pages.clear()
  state.pageIds.clear()
  state.refsByTarget.clear()
  state.pendingReadyTargets.clear()
  state.inflightRequestsByTarget.clear()
  state.activeTargetId = null
}

function clearConnectedSession(state) {
  state.browser = null
  state.context = null
  state.connectionMode = null
  clearProfilePages(state)
}

function markSessionDisconnected(state) {
  clearConnectedSession(state)
  state.launchedProcess = null
  state.launchedCdpUrl = null
}

function hasLiveContext(state) {
  if (!state?.context) return false
  if (state.connectionMode === 'cdp' && state.browser && typeof state.browser.isConnected === 'function') {
    if (!state.browser.isConnected()) {
      return false
    }
  }
  try {
    state.context.pages()
    return true
  } catch {
    return false
  }
}

async function attachExistingBrowserViaCdp(profileConfig, browserConfig, state) {
  const cdpUrl = typeof profileConfig?.cdp_url === 'string' ? profileConfig.cdp_url.trim() : ''
  if (!cdpUrl) {
    throw new Error('cdp_url is required for attach mode')
  }
  await terminateOwnedBrowserProcess(state)
  await attachBrowserViaCdp(cdpUrl, browserConfig, state, null)
}

async function attachBrowserViaCdp(cdpUrl, browserConfig, state, launchedProcess) {
  const browser = await chromium.connectOverCDP(cdpUrl)
  const contexts = browser.contexts()
  const context = contexts[0] || null
  if (!context) {
    throw new Error(`No context found after connecting to CDP endpoint: ${cdpUrl}`)
  }
  state.browser = browser
  state.context = context
  state.connectionMode = 'cdp'
  state.launchedProcess = launchedProcess
  state.launchedCdpUrl = launchedProcess ? cdpUrl : null
  clearProfilePages(state)

  const onSessionClosed = () => {
    markSessionDisconnected(state)
  }
  browser.on('disconnected', onSessionClosed)
  context.on('close', onSessionClosed)

  for (const page of context.pages()) {
    registerPage(state, page)
  }
  context.on('page', (page) => {
    const targetId = registerPage(state, page)
    void maybeActivatePageFromOpener(state, targetId, page)
  })

  if (state.headers && typeof state.headers === 'object') {
    await context.setExtraHTTPHeaders(state.headers).catch(() => undefined)
  }
  if (state.geolocation) {
    await context.setGeolocation(state.geolocation).catch(() => undefined)
    await context.grantPermissions(['geolocation']).catch(() => undefined)
  }
  if (state.media) {
    for (const page of context.pages()) {
      await page.emulateMedia({ colorScheme: state.media }).catch(() => undefined)
    }
  }
  if (browserConfig?.allow_private_network === false) {
    context.route('**', async (route) => {
      try {
        const url = route.request().url()
        assertPrivateNetworkAllowed(url, browserConfig.allow_private_network)
      } catch {
        return route.abort()
      }
      return route.continue()
    }).catch(() => undefined)
  }
  if (context.pages().length === 0) {
    const page = await context.newPage()
    registerPage(state, page)
    await page.goto('about:blank').catch(() => undefined)
  }
}

function buildChromeLaunchArgs(profileConfig, userDataDir, remoteDebuggingPort) {
  const viewport = defaultViewport(profileConfig)
  const args = [
    `--remote-debugging-port=${remoteDebuggingPort}`,
    '--remote-debugging-address=127.0.0.1',
    `--user-data-dir=${userDataDir}`,
    '--no-first-run',
    '--no-default-browser-check',
    '--new-window',
    `--window-size=${viewport.width},${viewport.height}`
  ]
  if (profileConfig?.headless) {
    args.push('--headless=new')
  }
  args.push('about:blank')
  return args
}

async function launchExternalChromeViaCdp(profileName, profileConfig, browserConfig, state, paths) {
  const executablePath = typeof profileConfig?.executable_path === 'string'
    ? profileConfig.executable_path.trim()
    : ''
  if (!executablePath) {
    throw new Error(`Profile "${profileName}" requires executable_path for launch mode`)
  }

  const userDataDir = (typeof profileConfig?.user_data_dir === 'string' && profileConfig.user_data_dir.trim())
    ? profileConfig.user_data_dir.trim()
    : profileUserDataDir(paths, profileName)
  await ensureDir(userDataDir)

  const remoteDebuggingPort = await findFreeTcpPort()
  const cdpUrl = `http://127.0.0.1:${remoteDebuggingPort}`
  const launchArgs = buildChromeLaunchArgs(profileConfig, userDataDir, remoteDebuggingPort)

  const child = spawn(executablePath, launchArgs, {
    stdio: 'ignore',
    windowsHide: true,
    detached: true
  })
  child.unref()

  try {
    const cdpReady = await waitForCdpEndpoint(cdpUrl)
    const connectEndpoint =
      typeof cdpReady?.webSocketDebuggerUrl === 'string' && cdpReady.webSocketDebuggerUrl
        ? cdpReady.webSocketDebuggerUrl
        : cdpUrl
    await attachBrowserViaCdp(connectEndpoint, browserConfig, state, child)
  } catch (error) {
    await terminateChildProcess(child)
    throw new Error(
      `Failed to launch browser with remote debugging (${executablePath}): ${makeErrorMessage(error)}`
    )
  }
}

function attachPageListeners(state, page, targetId) {
  const bumpInflight = (delta) => {
    const current = Number(state.inflightRequestsByTarget.get(targetId) || 0)
    const next = Math.max(0, current + delta)
    state.inflightRequestsByTarget.set(targetId, next)
  }

  page.on('console', (msg) => {
    pushLimited(state.consoleMessages, {
      level: msg.type(),
      text: msg.text(),
      target_id: targetId,
      ts: nowIso()
    })
  })

  page.on('pageerror', (error) => {
    pushLimited(state.errors, {
      message: makeErrorMessage(error),
      target_id: targetId,
      ts: nowIso()
    })
  })

  page.on('request', (request) => {
    bumpInflight(1)
    pushLimited(state.requests, {
      url: request.url(),
      method: request.method(),
      resource_type: request.resourceType(),
      target_id: targetId,
      ts: nowIso()
    })
  })

  page.on('requestfinished', () => {
    bumpInflight(-1)
  })

  page.on('requestfailed', () => {
    bumpInflight(-1)
  })

  page.on('response', async (response) => {
    if (!state.captureResponseBodies) return
    try {
      const request = response.request()
      const url = response.url()
      const status = response.status()
      let body = ''
      try {
        body = await response.text()
      } catch {
        body = ''
      }
      if (body.length > 50_000) body = body.slice(0, 50_000)
      pushLimited(state.responseBodies, {
        url,
        status,
        method: request.method(),
        body,
        target_id: targetId,
        ts: nowIso()
      }, 100)
    } catch {
      // ignore response capture failures
    }
  })

  page.on('popup', (popupPage) => {
    const popupTargetId = registerPage(state, popupPage)
    if (state.activeTargetId !== targetId) return
    void activateTargetPage(state, popupTargetId, popupPage)
  })

  page.on('close', () => {
    state.pages.delete(targetId)
    state.pageIds.delete(page)
    state.refsByTarget.delete(targetId)
    state.pendingReadyTargets.delete(targetId)
    state.inflightRequestsByTarget.delete(targetId)
    if (state.activeTargetId === targetId) {
      const next = Array.from(state.pages.keys())[0] || null
      state.activeTargetId = next
    }
  })
}

async function activateTargetPage(state, targetId, page) {
  if (!targetId || !page) return
  state.activeTargetId = targetId
  markTargetNeedsReady(state, targetId)
  await page.bringToFront().catch(() => undefined)
}

async function maybeActivatePageFromOpener(state, targetId, page) {
  try {
    const opener = await page.opener()
    if (!opener) return
    const openerTargetId = state.pageIds.get(opener)
    if (!openerTargetId || openerTargetId !== state.activeTargetId) return
    await activateTargetPage(state, targetId, page)
  } catch {
    // ignore opener checks for pages where this is unavailable
  }
}

function registerPage(state, page) {
  const existing = state.pageIds.get(page)
  if (existing) return existing
  const targetId = `t${state.nextTargetSeq++}`
  state.pages.set(targetId, page)
  state.pageIds.set(page, targetId)
  state.inflightRequestsByTarget.set(targetId, 0)
  if (!state.activeTargetId) state.activeTargetId = targetId
  attachPageListeners(state, page, targetId)
  return targetId
}

function resolveTargetId(state, request) {
  const requested = typeof request.target_id === 'string' ? request.target_id.trim() : ''
  if (requested) return requested
  if (state.activeTargetId) return state.activeTargetId
  const first = Array.from(state.pages.keys())[0]
  if (!first) throw new Error('No active browser tab. Use action=open first.')
  return first
}

function resolvePage(state, request) {
  const targetId = resolveTargetId(state, request)
  const page = state.pages.get(targetId)
  if (!page) {
    throw new Error(`Tab not found for target_id=${targetId}`)
  }
  return { page, targetId }
}

async function ensureContext(profileName, profileConfig, browserConfig, paths) {
  const state = getProfileState(profileName)
  if (state.context && !hasLiveContext(state)) {
    clearConnectedSession(state)
    await terminateOwnedBrowserProcess(state)
  }
  if (!state.context) {
    const cdpUrl = typeof profileConfig?.cdp_url === 'string' ? profileConfig.cdp_url.trim() : ''
    if (cdpUrl) {
      await attachExistingBrowserViaCdp(profileConfig, browserConfig, state)
    } else {
      const executablePath = typeof profileConfig?.executable_path === 'string'
        ? profileConfig.executable_path.trim()
        : ''
      if (!executablePath) {
        throw new Error('Profile must set executable_path for external Chrome launch, or set cdp_url to attach an existing debug Chrome')
      }
      await launchExternalChromeViaCdp(profileName, profileConfig, browserConfig, state, paths)
    }
    return state
  }
  return state
}

async function closeProfile(profileName) {
  const state = runtime.profiles.get(profileName)
  if (!state) return
  if (!state.context) {
    await terminateOwnedBrowserProcess(state)
    return
  }
  if (state.connectionMode === 'cdp') {
    if (state.browser) {
      await state.browser.close().catch(() => undefined)
    } else {
      await state.context.close().catch(() => undefined)
    }
  } else {
    await state.context.close().catch(() => undefined)
  }
  clearConnectedSession(state)
  await terminateOwnedBrowserProcess(state)
}

function extractPageLinks(html, baseUrl, maxLinks = 30) {
  const regex = /href\s*=\s*["']([^"'#\s]+)["']/gi
  const links = []
  let match
  while ((match = regex.exec(html))) {
    const raw = match[1]
    let absolute = raw
    try {
      absolute = new URL(raw, baseUrl).toString()
    } catch {
      // keep raw
    }
    if (!links.includes(absolute)) {
      links.push(absolute)
    }
    if (links.length >= maxLinks) break
  }
  return links
}

function clampInteger(value, min, max, fallback) {
  if (!Number.isFinite(value)) return fallback
  const integer = Math.trunc(value)
  if (integer < min) return min
  if (integer > max) return max
  return integer
}

function sanitizeActTimeout(timeoutValue, fallback = 2500) {
  return clampInteger(Number(timeoutValue), 250, 20_000, fallback)
}

function normalizePerformancePreset(value) {
  const preset = typeof value === 'string' ? value.trim().toLowerCase() : ''
  if (preset === 'safe' || preset === 'balanced' || preset === 'fast') return preset
  return 'balanced'
}

function resolveActStrategy(params, browserConfig) {
  const explicit = typeof params?.strategy === 'string' ? params.strategy.trim().toLowerCase() : ''
  if (explicit === 'fast' || explicit === 'balanced' || explicit === 'robust') return explicit
  const preset = normalizePerformancePreset(browserConfig?.performance_preset)
  if (preset === 'safe') return 'robust'
  if (preset === 'fast') return 'fast'
  return 'balanced'
}

function resolveDefaultActTimeoutMs(browserConfig, strategy) {
  const configured = Number(browserConfig?.default_act_timeout_ms)
  if (Number.isFinite(configured)) {
    return sanitizeActTimeout(configured, 1400)
  }
  if (strategy === 'robust') return 2500
  if (strategy === 'fast') return 900
  return 1400
}

function resolveActTimeoutMs(params, browserConfig, strategy) {
  const configuredDefault = resolveDefaultActTimeoutMs(browserConfig, strategy)
  const timeoutRaw = params?.timeout_ms
  if (strategy === 'robust') {
    return sanitizeActTimeout(timeoutRaw, Math.max(1800, configuredDefault))
  }
  if (strategy === 'fast') {
    return sanitizeActTimeout(timeoutRaw, Math.min(configuredDefault, 1200))
  }
  return sanitizeActTimeout(timeoutRaw, configuredDefault)
}

function buildPerfMeta(perf = {}) {
  const durationMs = Number(perf.duration_ms)
  return {
    resolve_ms: Number(perf.resolve_ms) || 0,
    locate_ms: Number(perf.locate_ms) || 0,
    action_ms: Number(perf.action_ms) || 0,
    fallback_count: Number(perf.fallback_count) || 0,
    total_ms: Number.isFinite(durationMs) ? durationMs : 0
  }
}

function movePerfFromData(data) {
  if (!data || typeof data !== 'object' || Array.isArray(data)) {
    return { payload: data, perf: null }
  }
  const perf = data.__perf && typeof data.__perf === 'object' ? data.__perf : null
  if (!perf) return { payload: data, perf: null }
  const payload = { ...data }
  delete payload.__perf
  return { payload, perf }
}

function markTargetNeedsReady(state, targetId) {
  if (!targetId) return
  state.pendingReadyTargets.set(targetId, Date.now())
}

function targetNeedsReadyGate(state, targetId) {
  const ts = state.pendingReadyTargets.get(targetId)
  if (!ts) return false
  state.pendingReadyTargets.delete(targetId)
  return (Date.now() - ts) <= TARGET_READY_TTL_MS
}

function stageTimeouts(totalTimeoutMs, strategy) {
  const total = sanitizeActTimeout(totalTimeoutMs, 1400)
  if (strategy === 'fast') {
    const primary = clampInteger(total * 0.35, 120, total, 360)
    const semantic = clampInteger(total * 0.30, 120, total, 300)
    const fallback = clampInteger(total - primary - semantic, 120, total, 360)
    return { total, primary, semantic, fallback }
  }
  if (strategy === 'robust') {
    const primary = clampInteger(total * 0.45, 200, total, 1200)
    const semantic = clampInteger(total * 0.30, 180, total, 900)
    const fallback = clampInteger(total - primary - semantic, 180, total, 1000)
    return { total, primary, semantic, fallback }
  }
  const primary = clampInteger(total * 0.40, 150, total, 700)
  const semantic = clampInteger(total * 0.30, 150, total, 500)
  const fallback = clampInteger(total - primary - semantic, 150, total, 500)
  return { total, primary, semantic, fallback }
}

function uniqueStrings(values) {
  const seen = new Set()
  const result = []
  for (const value of values) {
    if (typeof value !== 'string') continue
    const trimmed = value.trim()
    if (!trimmed) continue
    if (seen.has(trimmed)) continue
    seen.add(trimmed)
    result.push(trimmed)
  }
  return result
}

function compactSnapshotEntry(row) {
  return {
    ref: row.ref,
    role: row.role || row.tag || 'element',
    tag: row.tag || null,
    type: row.type || null,
    name: row.name || '',
    text: row.text || '',
    selector: row.selector,
    bbox: row.bbox || null,
    in_viewport: Boolean(row.in_viewport),
    disabled: Boolean(row.disabled)
  }
}

function resolveRefEntry(state, targetId, ref) {
  const map = state.refsByTarget.get(targetId)
  if (!map) return null
  return map.get(ref) || null
}

function selectorCandidatesFromRef(selector, refEntry) {
  const selectors = []
  if (typeof selector === 'string' && selector.trim()) {
    selectors.push(selector.trim())
  }
  if (!refEntry) return uniqueStrings(selectors)
  if (typeof refEntry === 'string') {
    selectors.push(refEntry)
    return uniqueStrings(selectors)
  }
  if (typeof refEntry.selector === 'string') {
    selectors.push(refEntry.selector)
  }
  if (Array.isArray(refEntry.fallback_selectors)) {
    selectors.push(...refEntry.fallback_selectors)
  }
  return uniqueStrings(selectors)
}

function roleHintForLocator(refEntry) {
  if (!refEntry || typeof refEntry !== 'object') return null
  const role = typeof refEntry.role === 'string' ? refEntry.role.toLowerCase() : ''
  const tag = typeof refEntry.tag === 'string' ? refEntry.tag.toLowerCase() : ''
  const type = typeof refEntry.type === 'string' ? refEntry.type.toLowerCase() : ''
  const supported = new Set([
    'button',
    'link',
    'checkbox',
    'radio',
    'textbox',
    'combobox',
    'option',
    'tab',
    'menuitem',
    'switch',
    'searchbox'
  ])
  if (supported.has(role)) return role
  if (tag === 'a') return 'link'
  if (tag === 'button') return 'button'
  if (tag === 'select') return 'combobox'
  if (tag === 'textarea') return 'textbox'
  if (tag === 'input') {
    if (type === 'checkbox') return 'checkbox'
    if (type === 'radio') return 'radio'
    if (type === 'search') return 'searchbox'
    if (['button', 'submit', 'reset'].includes(type)) return 'button'
    return 'textbox'
  }
  return null
}

async function clickLocator(locator, params, timeoutMs, options = {}) {
  if (options.scroll !== false) {
    await locator.scrollIntoViewIfNeeded({ timeout: timeoutMs }).catch(() => undefined)
  }
  await locator.click({
    button: typeof params.button === 'string' ? params.button : 'left',
    clickCount: params.double ? 2 : 1,
    timeout: timeoutMs
  })
}

async function typeLocator(locator, params, timeoutMs, options = {}) {
  if (options.scroll !== false) {
    await locator.scrollIntoViewIfNeeded({ timeout: timeoutMs }).catch(() => undefined)
  }
  if (params.replace !== false) {
    await locator.fill(String(params.text || ''), { timeout: timeoutMs })
  } else {
    await locator.type(String(params.text || ''), { timeout: timeoutMs })
  }
}

function timeLocate(timing, fn) {
  const started = Date.now()
  const value = fn()
  timing.locate_ms += Date.now() - started
  return value
}

async function timeAction(timing, fn) {
  const started = Date.now()
  const result = await fn()
  timing.action_ms += Date.now() - started
  return result
}

async function robustClick(page, selectors, refEntry, params, strategyConfig, timing) {
  const timeoutPrimary = strategyConfig.primary
  const timeoutSemantic = strategyConfig.semantic
  const timeoutFallback = strategyConfig.fallback
  const attempts = []

  for (let index = 0; index < selectors.length; index += 1) {
    const selector = selectors[index]
    const timeoutMs = index === 0 ? timeoutPrimary : timeoutFallback
    try {
      const locator = timeLocate(timing, () => page.locator(selector).first())
      await timeAction(timing, () => clickLocator(locator, params, timeoutMs, { scroll: false }))
      return { method: 'selector', selector, attempts, fallback_count: attempts.length }
    } catch (error) {
      attempts.push(`selector:${selector} => ${makeErrorMessage(error)}`)
    }
  }

  const nameHint = typeof refEntry?.name === 'string' && refEntry.name.trim()
    ? refEntry.name.trim()
    : (typeof refEntry?.text === 'string' ? refEntry.text.trim() : '')
  const roleHint = roleHintForLocator(refEntry)

  if (roleHint && nameHint) {
    try {
      const locator = timeLocate(timing, () => page.getByRole(roleHint, { name: nameHint, exact: false }).first())
      await timeAction(timing, () => clickLocator(locator, params, timeoutSemantic, { scroll: true }))
      return { method: 'role_name', role: roleHint, name: nameHint, attempts, fallback_count: attempts.length }
    } catch (error) {
      attempts.push(`role:${roleHint}(${nameHint}) => ${makeErrorMessage(error)}`)
    }
  }

  if (nameHint) {
    try {
      const locator = timeLocate(timing, () => page.getByLabel(nameHint, { exact: false }).first())
      await timeAction(timing, () => clickLocator(locator, params, timeoutSemantic, { scroll: true }))
      return { method: 'label', label: nameHint, attempts, fallback_count: attempts.length }
    } catch (error) {
      attempts.push(`label:${nameHint} => ${makeErrorMessage(error)}`)
    }
  }

  if (params.allow_coordinate_fallback === true && refEntry?.bbox && Number.isFinite(refEntry.bbox.y)) {
    try {
      await timeAction(timing, () => page.evaluate((y) => {
        const target = Math.max(0, Number(y) - window.innerHeight * 0.35)
        window.scrollTo({ top: target, behavior: 'auto' })
      }, refEntry.bbox.y))
      const viewport = await timeAction(timing, () => page.evaluate(() => ({ w: window.innerWidth, h: window.innerHeight })))
      const rawX = Number(refEntry.bbox.cx ?? (refEntry.bbox.x + refEntry.bbox.width / 2))
      const rawY = Number(refEntry.bbox.cy ?? (refEntry.bbox.y + refEntry.bbox.height / 2))
      const x = Math.max(1, Math.min(Math.max(1, viewport.w - 1), rawX))
      const y = Math.max(1, Math.min(Math.max(1, viewport.h - 1), rawY))
      await timeAction(timing, () => page.mouse.click(x, y, {
        button: typeof params.button === 'string' ? params.button : 'left',
        clickCount: params.double ? 2 : 1
      }))
      return { method: 'coordinates', x, y, attempts, fallback_count: attempts.length }
    } catch (error) {
      attempts.push(`coordinates => ${makeErrorMessage(error)}`)
    }
  }

  for (const selector of selectors) {
    try {
      const clicked = await timeAction(timing, () => page.evaluate((candidate) => {
        try {
          const element = document.querySelector(candidate)
          if (!element) return false
          element.scrollIntoView({ block: 'center', inline: 'center', behavior: 'auto' })
          if (typeof element.click === 'function') {
            element.click()
            return true
          }
          element.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }))
          return true
        } catch {
          return false
        }
      }, selector))
      if (clicked) {
        return { method: 'dom_click', selector, attempts, fallback_count: attempts.length }
      }
      attempts.push(`dom_click:${selector} => element_not_found`)
    } catch (error) {
      attempts.push(`dom_click:${selector} => ${makeErrorMessage(error)}`)
    }
  }

  const lastErrors = attempts.slice(-6).join(' | ')
  throw new Error(`act.click failed after ${attempts.length} attempts${lastErrors ? `: ${lastErrors}` : ''}`)
}

async function robustType(page, selectors, refEntry, params, strategyConfig, timing) {
  const timeoutPrimary = strategyConfig.primary
  const timeoutSemantic = strategyConfig.semantic
  const timeoutFallback = strategyConfig.fallback
  const attempts = []

  for (let index = 0; index < selectors.length; index += 1) {
    const selector = selectors[index]
    const timeoutMs = index === 0 ? timeoutPrimary : timeoutFallback
    try {
      const locator = timeLocate(timing, () => page.locator(selector).first())
      await timeAction(timing, () => typeLocator(locator, params, timeoutMs, { scroll: false }))
      if (params.submit) {
        await timeAction(timing, () => page.keyboard.press('Enter'))
      }
      return { method: 'selector', selector, attempts, fallback_count: attempts.length }
    } catch (error) {
      attempts.push(`selector:${selector} => ${makeErrorMessage(error)}`)
    }
  }

  const nameHint = typeof refEntry?.name === 'string' ? refEntry.name.trim() : ''
  if (nameHint) {
    try {
      const locator = timeLocate(timing, () => page.getByLabel(nameHint, { exact: false }).first())
      await timeAction(timing, () => typeLocator(locator, params, timeoutSemantic, { scroll: true }))
      if (params.submit) {
        await timeAction(timing, () => page.keyboard.press('Enter'))
      }
      return { method: 'label', label: nameHint, attempts, fallback_count: attempts.length }
    } catch (error) {
      attempts.push(`label:${nameHint} => ${makeErrorMessage(error)}`)
    }
  }

  for (const selector of selectors) {
    try {
      const changed = await timeAction(timing, () => page.evaluate(
        ({ candidate, text, replace }) => {
          try {
            const el = document.querySelector(candidate)
            if (!el) return false
            if (!(el instanceof HTMLInputElement || el instanceof HTMLTextAreaElement)) return false
            el.focus()
            if (replace !== false) {
              el.value = text
            } else {
              el.value += text
            }
            el.dispatchEvent(new Event('input', { bubbles: true }))
            el.dispatchEvent(new Event('change', { bubbles: true }))
            return true
          } catch {
            return false
          }
        },
        { candidate: selector, text: String(params.text || ''), replace: params.replace }
      ))
      if (!changed) {
        attempts.push(`dom_type:${selector} => element_not_found`)
        continue
      }
      if (params.submit) {
        await timeAction(timing, () => page.keyboard.press('Enter'))
      }
      return { method: 'dom_type', selector, attempts, fallback_count: attempts.length }
    } catch (error) {
      attempts.push(`dom_type:${selector} => ${makeErrorMessage(error)}`)
    }
  }

  const lastErrors = attempts.slice(-6).join(' | ')
  throw new Error(`act.type failed after ${attempts.length} attempts${lastErrors ? `: ${lastErrors}` : ''}`)
}

async function robustHover(page, selectors, params) {
  const timeoutMs = sanitizeActTimeout(params.timeout_ms, 2500)
  const attempts = []
  for (const selector of selectors) {
    try {
      const locator = page.locator(selector).first()
      await locator.scrollIntoViewIfNeeded({ timeout: timeoutMs }).catch(() => undefined)
      await locator.hover({ timeout: timeoutMs })
      return { method: 'selector', selector, attempts }
    } catch (error) {
      attempts.push(`selector:${selector} => ${makeErrorMessage(error)}`)
    }
  }
  const lastErrors = attempts.slice(-6).join(' | ')
  throw new Error(`act.hover failed after ${attempts.length} attempts${lastErrors ? `: ${lastErrors}` : ''}`)
}

async function robustSelect(page, selectors, params) {
  const timeoutMs = sanitizeActTimeout(params.timeout_ms, 2500)
  const attempts = []
  for (const selector of selectors) {
    try {
      const locator = page.locator(selector).first()
      await locator.scrollIntoViewIfNeeded({ timeout: timeoutMs }).catch(() => undefined)
      await locator.selectOption(params.values || params.value || [], { timeout: timeoutMs })
      return { method: 'selector', selector, attempts }
    } catch (error) {
      attempts.push(`selector:${selector} => ${makeErrorMessage(error)}`)
    }
  }
  const lastErrors = attempts.slice(-6).join(' | ')
  throw new Error(`act.select failed after ${attempts.length} attempts${lastErrors ? `: ${lastErrors}` : ''}`)
}

async function hasVisibleLoadingIndicators(page) {
  return await page.evaluate(() => {
    const selectors = [
      '[class*="spinner"]',
      '[class*="loading"]',
      '[class*="loader"]',
      '[class*="skeleton"]',
      '[class*="progress"]',
      '[aria-busy="true"]',
      '[role="progressbar"]'
    ]
    for (const selector of selectors) {
      const nodes = document.querySelectorAll(selector)
      for (const node of nodes) {
        if (!(node instanceof HTMLElement)) continue
        const style = getComputedStyle(node)
        const rect = node.getBoundingClientRect()
        const visible = style.display !== 'none' &&
          style.visibility !== 'hidden' &&
          style.opacity !== '0' &&
          rect.width > 0 &&
          rect.height > 0
        if (visible) return true
      }
    }
    return false
  })
}

async function waitForLoadingIndicatorsToDisappear(page, timeoutMs) {
  const started = Date.now()
  while ((Date.now() - started) < timeoutMs) {
    const hasLoadingIndicators = await hasVisibleLoadingIndicators(page)
    if (!hasLoadingIndicators) return { ok: true, duration_ms: Date.now() - started }
    await page.waitForTimeout(80)
  }
  return { ok: false, duration_ms: Date.now() - started, reason: 'loading_timeout' }
}

async function waitForTargetNetworkIdle(state, targetId, timeoutMs, stableMs) {
  const started = Date.now()
  let idleStarted = null
  while ((Date.now() - started) < timeoutMs) {
    const inflight = Number(state.inflightRequestsByTarget.get(targetId) || 0)
    if (inflight === 0) {
      if (idleStarted == null) {
        idleStarted = Date.now()
      } else if ((Date.now() - idleStarted) >= stableMs) {
        return { ok: true, duration_ms: Date.now() - started }
      }
    } else {
      idleStarted = null
    }
    await new Promise((resolve) => setTimeout(resolve, 40))
  }
  return { ok: false, duration_ms: Date.now() - started, reason: 'network_idle_timeout' }
}

async function waitForDomStable(page, stableMs, timeoutMs) {
  const started = Date.now()
  try {
    await page.evaluate(
      ({ stable, timeout }) => new Promise((resolve) => {
        const root = document.body || document.documentElement
        if (!root) {
          resolve(true)
          return
        }
        let done = false
        let timer = null
        let stableTimer = null
        const cleanup = () => {
          done = true
          if (timer) clearTimeout(timer)
          if (stableTimer) clearTimeout(stableTimer)
          observer.disconnect()
        }
        const settle = () => {
          if (done) return
          cleanup()
          resolve(true)
        }
        const rearm = () => {
          if (done) return
          if (stableTimer) clearTimeout(stableTimer)
          stableTimer = setTimeout(settle, stable)
        }
        const observer = new MutationObserver((mutations) => {
          if (done) return
          const significant = mutations.some((mutation) => {
            if (mutation.type === 'childList') return true
            if (mutation.type === 'characterData') return true
            if (mutation.type === 'attributes' && mutation.target instanceof HTMLElement) {
              const rect = mutation.target.getBoundingClientRect()
              return rect.width > 0 && rect.height > 0
            }
            return false
          })
          if (significant) rearm()
        })
        observer.observe(root, { childList: true, subtree: true, attributes: true, characterData: true })
        timer = setTimeout(settle, timeout)
        rearm()
      }),
      { stable: stableMs, timeout: timeoutMs }
    )
    return { ok: true, duration_ms: Date.now() - started }
  } catch {
    return { ok: false, duration_ms: Date.now() - started, reason: 'dom_stability_error' }
  }
}

function recordReadyCheckFailure(state, targetId, phase, detail) {
  if (!detail || detail.ok !== false) return
  pushLimited(state.errors, {
    message: `[ready_check:${phase}] ${detail.reason || 'failed'}`,
    target_id: targetId,
    ts: nowIso()
  })
}

async function waitForPageReadyBeforeAct(state, targetId, page) {
  const loadingVisible = await hasVisibleLoadingIndicators(page).catch(() => false)
  const inflightAtStart = Number(state.inflightRequestsByTarget.get(targetId) || 0)

  const loadingResult = loadingVisible
    ? await waitForLoadingIndicatorsToDisappear(page, PAGE_READY_LOADING_TIMEOUT_MS).catch(() => ({
      ok: false,
      duration_ms: PAGE_READY_LOADING_TIMEOUT_MS,
      reason: 'loading_check_error'
    }))
    : { ok: true, duration_ms: 0, skipped: true }

  const networkResult = inflightAtStart > 0
    ? await waitForTargetNetworkIdle(
      state,
      targetId,
      PAGE_READY_NETWORK_IDLE_TIMEOUT_MS,
      PAGE_READY_NETWORK_IDLE_STABLE_MS
    ).catch(() => ({
      ok: false,
      duration_ms: PAGE_READY_NETWORK_IDLE_TIMEOUT_MS,
      reason: 'network_idle_error'
    }))
    : { ok: true, duration_ms: 0, skipped: true }

  const shouldCheckDomStability = loadingVisible || inflightAtStart > 0
  const domResult = shouldCheckDomStability
    ? await waitForDomStable(
      page,
      PAGE_READY_DOM_STABLE_MS,
      PAGE_READY_DOM_STABILITY_TIMEOUT_MS
    ).catch(() => ({
      ok: false,
      duration_ms: PAGE_READY_DOM_STABILITY_TIMEOUT_MS,
      reason: 'dom_stability_timeout'
    }))
    : { ok: true, duration_ms: 0, skipped: true }

  return {
    loading: loadingResult,
    network_idle: networkResult,
    dom_stability: domResult
  }
}

async function buildSnapshot(state, request) {
  const { page, targetId } = resolvePage(state, request)
  const modeRaw = typeof request.params?.mode === 'string' ? request.params.mode.trim().toLowerCase() : ''
  const mode = modeRaw === 'full' ? 'full' : 'compact'
  const compactMode = mode === 'compact'
  const defaultRefs = compactMode ? DEFAULT_COMPACT_SNAPSHOT_REFS : DEFAULT_SNAPSHOT_MAX_REFS
  const candidateLimit = compactMode ? 1000 : 1500
  const maxLimit = compactMode ? 300 : MAX_SNAPSHOT_MAX_REFS
  const maxRefs = clampInteger(
    Number(request.params?.max_refs),
    20,
    maxLimit,
    defaultRefs
  )
  const snapshotResult = await page.evaluate(({ markerAttr, maxRefs, candidateLimit, mode }) => {
    function normalizeText(value, limit = 160) {
      return String(value || '').replace(/\s+/g, ' ').trim().slice(0, limit)
    }

    function cssPath(el) {
      if (!(el instanceof Element)) return null
      const parts = []
      let current = el
      while (current && current.nodeType === 1 && parts.length < 6) {
        let selector = current.nodeName.toLowerCase()
        if (current.id) {
          selector += `#${CSS.escape(current.id)}`
          parts.unshift(selector)
          break
        }
        const classList = Array.from(current.classList || []).slice(0, 2)
        if (classList.length > 0) {
          selector += classList.map((c) => `.${CSS.escape(c)}`).join('')
        }
        let sibling = current
        let nth = 1
        while ((sibling = sibling.previousElementSibling)) {
          if (sibling.nodeName === current.nodeName) nth += 1
        }
        selector += `:nth-of-type(${nth})`
        parts.unshift(selector)
        current = current.parentElement
      }
      return parts.join(' > ')
    }

    function attrSelector(attrName, attrValue, tagName = '') {
      if (!attrValue) return null
      const escaped = String(attrValue)
        .replace(/\\/g, '\\\\')
        .replace(/"/g, '\\"')
      return `${tagName || ''}[${attrName}="${escaped}"]`
    }

    function isSelectorUnique(selector, element) {
      if (!selector) return false
      try {
        const matches = document.querySelectorAll(selector)
        return matches.length === 1 && matches[0] === element
      } catch {
        return false
      }
    }

    function inferRole(tag, explicitRole, type) {
      if (explicitRole) return explicitRole
      if (tag === 'a') return 'link'
      if (tag === 'button') return 'button'
      if (tag === 'select') return 'combobox'
      if (tag === 'textarea') return 'textbox'
      if (tag === 'input') {
        if (type === 'checkbox') return 'checkbox'
        if (type === 'radio') return 'radio'
        if (type === 'search') return 'searchbox'
        if (['button', 'submit', 'reset'].includes(type)) return 'button'
        return 'textbox'
      }
      return tag
    }

    function isInteractiveRole(role) {
      return new Set([
        'button',
        'link',
        'menuitem',
        'option',
        'radio',
        'checkbox',
        'tab',
        'textbox',
        'combobox',
        'searchbox',
        'switch'
      ]).has(role)
    }

    function isInteractiveTag(tag) {
      return new Set([
        'a',
        'button',
        'input',
        'textarea',
        'select',
        'summary',
        'option'
      ]).has(tag)
    }

    function isVisible(el) {
      const style = getComputedStyle(el)
      if (style.display === 'none' || style.visibility === 'hidden') return false
      if (Number(style.opacity || '1') <= 0) return false
      if (style.pointerEvents === 'none') return false
      const rect = el.getBoundingClientRect()
      if (rect.width < 2 || rect.height < 2) return false
      if (rect.right < 0 || rect.bottom < 0) return false
      if (rect.left > window.innerWidth || rect.top > window.innerHeight) return false
      return true
    }

    document.querySelectorAll(`[${markerAttr}]`).forEach((node) => node.removeAttribute(markerAttr))

    const rawCandidates = Array.from(
      document.querySelectorAll(
        'a,button,input,textarea,select,summary,label,[role],[tabindex],[onclick],[aria-label],[data-testid],[contenteditable=""],[contenteditable="true"]'
      )
    ).slice(0, candidateLimit)

    const entries = []
    for (const el of rawCandidates) {
      if (!(el instanceof HTMLElement)) continue
      const tag = el.tagName.toLowerCase()
      const type = tag === 'input'
        ? String(el.getAttribute('type') || 'text').toLowerCase()
        : null
      if (tag === 'input' && type === 'hidden') continue
      if (!isVisible(el)) continue

      const explicitRole = (el.getAttribute('role') || '').toLowerCase()
      const role = inferRole(tag, explicitRole, type)
      const style = getComputedStyle(el)
      const rect = el.getBoundingClientRect()
      const text = normalizeText(el.innerText || el.textContent || '')
      const placeholder = normalizeText(el.getAttribute('placeholder') || '', 80)
      const ariaLabel = normalizeText(el.getAttribute('aria-label') || '', 120)
      const title = normalizeText(el.getAttribute('title') || '', 120)
      let label = ariaLabel || placeholder || title
      if (!label && tag === 'input') {
        try {
          const firstLabel = el.labels && el.labels.length > 0 ? el.labels[0] : null
          if (firstLabel) {
            label = normalizeText(firstLabel.innerText || firstLabel.textContent || '', 120)
          }
        } catch {
          // ignore label resolution failures
        }
      }
      if (!label) {
        label = text
      }
      const inViewport = rect.left < window.innerWidth &&
        rect.right > 0 &&
        rect.top < window.innerHeight &&
        rect.bottom > 0
      const disabled = el.matches(':disabled') || el.getAttribute('aria-disabled') === 'true'
      const hasInteractiveAttr = el.hasAttribute('onclick') ||
        el.hasAttribute('tabindex') ||
        el.getAttribute('contenteditable') === 'true' ||
        el.getAttribute('contenteditable') === ''
      const hasPointerCursor = style.cursor === 'pointer'
      const interactive = isInteractiveTag(tag) || isInteractiveRole(role) || hasInteractiveAttr || hasPointerCursor
      if (!interactive) continue

      const fallbackSelectors = []
      const idValue = el.getAttribute('id')
      if (idValue) {
        const idSelector = `#${CSS.escape(idValue)}`
        if (isSelectorUnique(idSelector, el)) fallbackSelectors.push(idSelector)
      }
      const dataTestId = el.getAttribute('data-testid')
      if (dataTestId) {
        const selector = attrSelector('data-testid', dataTestId)
        if (selector && isSelectorUnique(selector, el)) fallbackSelectors.push(selector)
      }
      const nameAttr = el.getAttribute('name')
      if (nameAttr) {
        const selector = attrSelector('name', nameAttr, tag)
        if (selector && isSelectorUnique(selector, el)) fallbackSelectors.push(selector)
      }
      if (ariaLabel) {
        const selector = attrSelector('aria-label', ariaLabel, tag)
        if (selector && isSelectorUnique(selector, el)) fallbackSelectors.push(selector)
      }
      const structuralSelector = cssPath(el)
      if (structuralSelector) fallbackSelectors.push(structuralSelector)

      let score = 0
      if (inViewport) score += 20
      if (isInteractiveTag(tag)) score += 12
      if (isInteractiveRole(role)) score += 8
      if (hasPointerCursor) score += 4
      if (label) score += Math.min(6, Math.ceil(label.length / 24))
      if (disabled) score -= 40
      score += Math.min(8, Math.floor((rect.width * rect.height) / 4000))

      entries.push({
        element: el,
        score,
        role,
        tag,
        type,
        text,
        name: label,
        placeholder,
        in_viewport: inViewport,
        disabled,
        bbox: {
          x: Number(rect.left.toFixed(1)),
          y: Number(rect.top.toFixed(1)),
          width: Number(rect.width.toFixed(1)),
          height: Number(rect.height.toFixed(1)),
          cx: Number((rect.left + rect.width / 2).toFixed(1)),
          cy: Number((rect.top + rect.height / 2).toFixed(1))
        },
        fallback_selectors: fallbackSelectors
      })
    }

    entries.sort((a, b) => {
      if (b.score !== a.score) return b.score - a.score
      if (a.in_viewport !== b.in_viewport) return a.in_viewport ? -1 : 1
      if (a.bbox.y !== b.bbox.y) return a.bbox.y - b.bbox.y
      return a.bbox.x - b.bbox.x
    })

    const selected = entries.slice(0, maxRefs)
    const rows = selected.map((entry, index) => {
      const ref = `e${index + 1}`
      entry.element.setAttribute(markerAttr, ref)
      const selector = `[${markerAttr}="${ref}"]`
      return {
        ref,
        role: entry.role,
        tag: entry.tag,
        type: entry.type,
        name: entry.name,
        text: entry.text,
        placeholder: entry.placeholder,
        selector,
        fallback_selectors: Array.from(new Set([selector, ...entry.fallback_selectors])).slice(0, 6),
        bbox: entry.bbox,
        in_viewport: entry.in_viewport,
        disabled: entry.disabled
      }
    })

    return {
      rows,
      stats: {
        total_candidates: entries.length,
        viewport_count: entries.filter((entry) => entry.in_viewport).length,
        selected_count: rows.length,
        mode
      }
    }
  }, { markerAttr: SNAPSHOT_MARKER_ATTR, maxRefs, candidateLimit, mode })

  const refs = snapshotResult.rows || []
  const refMap = new Map(refs.map((row) => [row.ref, row]))
  state.refsByTarget.set(targetId, refMap)

  return {
    target_id: targetId,
    url: page.url(),
    title: await page.title().catch(() => ''),
    refs: refs.map(compactSnapshotEntry),
    stats: {
      mode,
      count: refs.length,
      total_candidates: snapshotResult.stats?.total_candidates || refs.length,
      viewport_count: snapshotResult.stats?.viewport_count || refs.length
    }
  }
}

function resolveSelectorFromRef(state, targetId, ref) {
  const entry = resolveRefEntry(state, targetId, ref)
  if (!entry) return null
  if (typeof entry === 'string') return entry
  if (typeof entry.selector === 'string' && entry.selector.trim()) return entry.selector
  if (Array.isArray(entry.fallback_selectors)) {
    const first = entry.fallback_selectors.find((value) => typeof value === 'string' && value.trim())
    return first || null
  }
  return null
}

async function executeAct(state, request, browserConfig, options = {}) {
  const started = Date.now()
  const perf = {
    resolve_ms: 0,
    locate_ms: 0,
    action_ms: 0,
    fallback_count: 0,
    duration_ms: 0
  }
  const resolveStarted = Date.now()
  const params = requireObject(request.params || {}, 'params')
  const { page, targetId } = resolvePage(state, request)
  const kind = typeof params.kind === 'string' ? params.kind.trim().toLowerCase() : ''
  if (!kind) throw new Error('act.kind is required')

  const strategy = resolveActStrategy(params, browserConfig)
  const timeoutMs = resolveActTimeoutMs(params, browserConfig, strategy)
  const strategyConfig = stageTimeouts(timeoutMs, strategy)
  const paramsWithResolvedTimeout = { ...params, timeout_ms: timeoutMs }

  const ref = typeof params.ref === 'string' ? params.ref : null
  const selector = typeof params.selector === 'string' ? params.selector : null
  const refEntry = ref ? resolveRefEntry(state, targetId, ref) : null
  if (ref && !selector && !refEntry) {
    throw new Error(`Unknown ref "${ref}". Call browser action=snapshot to refresh refs.`)
  }
  const selectorCandidates = selectorCandidatesFromRef(selector, refEntry)
  const primarySelector = selectorCandidates[0] || null

  if (!options.skip_ready_check && targetNeedsReadyGate(state, targetId)) {
    const readyState = await waitForPageReadyBeforeAct(state, targetId, page).catch(() => undefined)
    if (readyState) {
      recordReadyCheckFailure(state, targetId, 'loading', readyState.loading)
      recordReadyCheckFailure(state, targetId, 'network_idle', readyState.network_idle)
      recordReadyCheckFailure(state, targetId, 'dom_stability', readyState.dom_stability)
    }
  }
  perf.resolve_ms = Date.now() - resolveStarted

  switch (kind) {
    case 'click': {
      if (selectorCandidates.length === 0) throw new Error('act.click requires ref or selector')
      const timing = { locate_ms: 0, action_ms: 0 }
      const detail = await robustClick(page, selectorCandidates, refEntry, paramsWithResolvedTimeout, strategyConfig, timing)
      perf.locate_ms += timing.locate_ms
      perf.action_ms += timing.action_ms
      perf.fallback_count += detail.fallback_count || 0
      perf.duration_ms = Date.now() - started
      return {
        ok: true,
        action: 'click',
        target_id: targetId,
        selector: detail.selector || primarySelector,
        method: detail.method,
        strategy,
        __perf: perf
      }
    }
    case 'type': {
      if (selectorCandidates.length === 0) throw new Error('act.type requires ref or selector')
      const timing = { locate_ms: 0, action_ms: 0 }
      const detail = await robustType(page, selectorCandidates, refEntry, paramsWithResolvedTimeout, strategyConfig, timing)
      perf.locate_ms += timing.locate_ms
      perf.action_ms += timing.action_ms
      perf.fallback_count += detail.fallback_count || 0
      perf.duration_ms = Date.now() - started
      return {
        ok: true,
        action: 'type',
        target_id: targetId,
        selector: detail.selector || primarySelector,
        method: detail.method,
        strategy,
        __perf: perf
      }
    }
    case 'press':
      {
        const actionStarted = Date.now()
        await page.keyboard.press(String(params.key || 'Enter'))
        perf.action_ms += Date.now() - actionStarted
      }
      perf.duration_ms = Date.now() - started
      return { ok: true, action: 'press', target_id: targetId, strategy, __perf: perf }
    case 'hover':
      if (selectorCandidates.length === 0) throw new Error('act.hover requires ref or selector')
      {
        const hoverStarted = Date.now()
        const detail = await robustHover(page, selectorCandidates, paramsWithResolvedTimeout)
        perf.action_ms += Date.now() - hoverStarted
        perf.fallback_count += detail.attempts?.length || 0
        perf.duration_ms = Date.now() - started
        return {
          ok: true,
          action: 'hover',
          target_id: targetId,
          selector: detail.selector || primarySelector,
          method: detail.method,
          strategy,
          __perf: perf
        }
      }
    case 'scroll':
      {
        const actionStarted = Date.now()
        await page.evaluate(
          ({ x, y }) => window.scrollBy(Number(x) || 0, Number(y) || 0),
          { x: params.x, y: params.y }
        )
        perf.action_ms += Date.now() - actionStarted
        perf.duration_ms = Date.now() - started
        return { ok: true, action: 'scroll', target_id: targetId, strategy, __perf: perf }
      }
    case 'select':
      if (selectorCandidates.length === 0) throw new Error('act.select requires ref or selector')
      {
        const selectStarted = Date.now()
        const detail = await robustSelect(page, selectorCandidates, paramsWithResolvedTimeout)
        perf.action_ms += Date.now() - selectStarted
        perf.fallback_count += detail.attempts?.length || 0
        perf.duration_ms = Date.now() - started
        return {
          ok: true,
          action: 'select',
          target_id: targetId,
          selector: detail.selector || primarySelector,
          method: detail.method,
          strategy,
          __perf: perf
        }
      }
    case 'wait':
      {
        const actionStarted = Date.now()
        if (params.selector) {
          await page.waitForSelector(String(params.selector), {
            timeout: Number.isInteger(params.timeout_ms) ? params.timeout_ms : timeoutMs
          })
        } else if (params.url) {
          await page.waitForURL(String(params.url), {
            timeout: Number.isInteger(params.timeout_ms) ? params.timeout_ms : timeoutMs
          })
        } else {
          await page.waitForTimeout(Number.isInteger(params.timeout_ms) ? params.timeout_ms : 250)
        }
        perf.action_ms += Date.now() - actionStarted
        perf.duration_ms = Date.now() - started
        return { ok: true, action: 'wait', target_id: targetId, strategy, __perf: perf }
      }
    case 'drag': {
      const fromRef = typeof params.from_ref === 'string' ? params.from_ref : null
      const toRef = typeof params.to_ref === 'string' ? params.to_ref : null
      const fromSelector = fromRef ? resolveSelectorFromRef(state, targetId, fromRef) : null
      const toSelector = toRef ? resolveSelectorFromRef(state, targetId, toRef) : null
      if (!fromSelector || !toSelector) throw new Error('act.drag requires from_ref and to_ref')
      const actionStarted = Date.now()
      await page.dragAndDrop(fromSelector, toSelector)
      perf.action_ms += Date.now() - actionStarted
      perf.duration_ms = Date.now() - started
      return { ok: true, action: 'drag', target_id: targetId, strategy, __perf: perf }
    }
    default:
      throw new Error(`Unsupported act.kind: ${kind}`)
  }
}

async function executeActBatch(state, request, browserConfig) {
  const params = requireObject(request.params || {}, 'params')
  if (!Array.isArray(params.actions) || params.actions.length === 0) {
    throw new Error('act_batch requires params.actions (non-empty array)')
  }
  const stopOnError = params.stop_on_error !== false
  const results = []
  const perf = {
    resolve_ms: 0,
    locate_ms: 0,
    action_ms: 0,
    fallback_count: 0,
    duration_ms: 0
  }
  const started = Date.now()
  for (let index = 0; index < params.actions.length; index += 1) {
    const actionParams = requireObject(params.actions[index], `params.actions[${index}]`)
    const subRequest = {
      ...request,
      params: actionParams
    }
    try {
      const item = await executeAct(state, subRequest, browserConfig, { skip_ready_check: false })
      const itemPerf = item.__perf && typeof item.__perf === 'object' ? item.__perf : {}
      results.push({
        index,
        ok: true,
        action: item.action || actionParams.kind || 'act',
        target_id: item.target_id || request.target_id || null,
        method: item.method || null,
        selector: item.selector || null,
        meta: buildPerfMeta(itemPerf)
      })
      perf.resolve_ms += Number(itemPerf.resolve_ms) || 0
      perf.locate_ms += Number(itemPerf.locate_ms) || 0
      perf.action_ms += Number(itemPerf.action_ms) || 0
      perf.fallback_count += Number(itemPerf.fallback_count) || 0
    } catch (error) {
      results.push({
        index,
        ok: false,
        action: actionParams.kind || 'act',
        error: makeErrorMessage(error)
      })
      if (stopOnError) break
    }
  }
  perf.duration_ms = Date.now() - started
  return {
    ok: true,
    action: 'act_batch',
    results,
    __perf: perf
  }
}

async function writeTempFile(dir, prefix, extension, buffer) {
  await ensureDir(dir)
  const filePath = path.join(
    dir,
    `${prefix}-${Date.now()}-${Math.random().toString(16).slice(2)}.${extension}`
  )
  await fs.writeFile(filePath, buffer)
  return filePath
}

async function handleAction(params) {
  const payload = requireObject(params, 'params')
  const request = requireObject(payload.request, 'request')
  const browserConfig = requireObject(payload.browser_config, 'browser_config')
  const paths = requireObject(payload.paths, 'paths')
  const action = typeof request.action === 'string' ? request.action.trim() : ''
  if (!action) throw new Error('request.action is required')

  const profileName = resolveProfileName(request, browserConfig)
  const profileConfig = browserConfig.profiles?.[profileName]

  if (action === 'profiles') {
    return {
      profiles: Object.entries(browserConfig.profiles || {}).map(([name, cfg]) => ({
        name,
        engine: cfg.engine || 'chrome',
        cdp_url: cfg.cdp_url || null,
        executable_path: cfg.executable_path || null,
        headless: Boolean(cfg.headless),
        running: hasLiveContext(runtime.profiles.get(name)),
        mode: runtime.profiles.get(name)?.connectionMode || null
      })),
      default_profile: browserConfig.default_profile,
      performance_preset: normalizePerformancePreset(browserConfig.performance_preset),
      capture_response_bodies: Boolean(browserConfig.capture_response_bodies),
      default_act_timeout_ms: resolveDefaultActTimeoutMs(browserConfig, 'balanced')
    }
  }

  if (action === 'status') {
    const state = getProfileState(profileName)
    return {
      profile: profileName,
      running: hasLiveContext(state),
      mode: state.connectionMode,
      active_target_id: state.activeTargetId,
      tabs: Array.from(state.pages.entries()).map(([targetId, page]) => ({
        target_id: targetId,
        url: page.url()
      }))
    }
  }

  if (action === 'stop' && request.profile == null) {
    for (const [name] of runtime.profiles) {
      await closeProfile(name)
    }
    return { stopped: 'all' }
  }

  if (!profileConfig) {
    throw new Error(`Unknown browser profile: ${profileName}`)
  }

  const state = getProfileState(profileName)
  state.captureResponseBodies = Boolean(browserConfig.capture_response_bodies)

  if (action === 'start') {
    const hasCdp = typeof profileConfig?.cdp_url === 'string' && profileConfig.cdp_url.trim().length > 0
    const hasExecutable = typeof profileConfig?.executable_path === 'string' && profileConfig.executable_path.trim().length > 0
    if (!hasCdp && !hasExecutable) {
      throw new Error(
        `Profile "${profileName}" must set executable_path (launch external Chrome with remote debugging) or cdp_url (attach existing debug Chrome).`
      )
    }
    await ensureContext(profileName, profileConfig, browserConfig, paths)
    return { profile: profileName, running: true, mode: state.connectionMode }
  }

  if (action === 'stop') {
    await closeProfile(profileName)
    return { profile: profileName, running: false }
  }

  if (action === 'reset_profile') {
    if (profileConfig?.user_data_dir) {
      throw new Error('reset_profile is blocked when user_data_dir is explicitly configured')
    }
    await closeProfile(profileName)
    const profileDir = path.join(paths.profiles_root, profileName)
    await fs.rm(profileDir, { recursive: true, force: true })
    return { profile: profileName, reset: true }
  }

  if (action === 'set_timezone') {
    state.timezone = String(request.params?.timezone || '')
    await closeProfile(profileName)
    await ensureContext(profileName, profileConfig, browserConfig, paths)
    return { profile: profileName, timezone: state.timezone, restarted: true }
  }

  if (action === 'set_locale') {
    state.locale = String(request.params?.locale || '')
    await closeProfile(profileName)
    await ensureContext(profileName, profileConfig, browserConfig, paths)
    return { profile: profileName, locale: state.locale, restarted: true }
  }

  if (action === 'set_device') {
    state.device = String(request.params?.device || '')
    await closeProfile(profileName)
    await ensureContext(profileName, profileConfig, browserConfig, paths)
    return { profile: profileName, device: state.device, restarted: true }
  }

  await ensureContext(profileName, profileConfig, browserConfig, paths)

  if (action === 'tabs') {
    return {
      profile: profileName,
      tabs: await Promise.all(
        Array.from(state.pages.entries()).map(async ([targetId, page]) => ({
          target_id: targetId,
          url: page.url(),
          title: await page.title().catch(() => ''),
          active: targetId === state.activeTargetId
        }))
      )
    }
  }

  if (action === 'open') {
    const url = String(request.params?.url || request.params?.targetUrl || '')
    if (!url) throw new Error('open requires params.url')
    assertPrivateNetworkAllowed(url, browserConfig.allow_private_network)
    const page = await state.context.newPage()
    const targetId = registerPage(state, page)
    await page.goto(url, { waitUntil: 'domcontentloaded' })
    markTargetNeedsReady(state, targetId)
    state.activeTargetId = targetId
    return {
      target_id: targetId,
      url: page.url(),
      title: await page.title().catch(() => '')
    }
  }

  if (action === 'focus') {
    const { page, targetId } = resolvePage(state, request)
    await page.bringToFront().catch(() => undefined)
    state.activeTargetId = targetId
    markTargetNeedsReady(state, targetId)
    return { target_id: targetId, focused: true }
  }

  if (action === 'close') {
    const { page, targetId } = resolvePage(state, request)
    await page.close()
    return { target_id: targetId, closed: true }
  }

  if (action === 'navigate') {
    const { page, targetId } = resolvePage(state, request)
    const url = String(request.params?.url || '')
    if (!url) throw new Error('navigate requires params.url')
    assertPrivateNetworkAllowed(url, browserConfig.allow_private_network)
    const response = await page.goto(url, { waitUntil: 'domcontentloaded' })
    markTargetNeedsReady(state, targetId)
    const includeLinks = Boolean(request.params?.include_links)
    let links = []
    if (includeLinks) {
      const html = await page.content()
      links = extractPageLinks(html, page.url(), Number(request.params?.max_links) || 30)
    }
    return {
      target_id: targetId,
      url: page.url(),
      status: response?.status() || 200,
      content_type: response?.headers()?.['content-type'] || 'text/html',
      title: await page.title().catch(() => ''),
      links,
      content_truncated: false
    }
  }

  if (action === 'snapshot') {
    return await buildSnapshot(state, request)
  }

  if (action === 'screenshot') {
    const { page, targetId } = resolvePage(state, request)
    const fullPage = Boolean(request.params?.full_page)
    const outputPath = request.params?.path
      ? String(request.params.path)
      : await writeTempFile(paths.app_log_dir, 'browser-shot', 'png', Buffer.alloc(0))
    const bytes = await page.screenshot({ path: outputPath, fullPage })
    return {
      target_id: targetId,
      path: outputPath,
      bytes: bytes?.byteLength || 0
    }
  }

  if (action === 'act') {
    return await executeAct(state, request, browserConfig)
  }

  if (action === 'act_batch') {
    return await executeActBatch(state, request, browserConfig)
  }

  if (action === 'console') {
    const level = request.params?.level ? String(request.params.level) : null
    const rows = level
      ? state.consoleMessages.filter((item) => item.level === level)
      : state.consoleMessages
    return { items: rows }
  }

  if (action === 'errors') {
    return { items: state.errors }
  }

  if (action === 'requests') {
    const pattern = request.params?.filter ? String(request.params.filter) : ''
    const items = pattern
      ? state.requests.filter((item) => item.url.includes(pattern))
      : state.requests
    return { items }
  }

  if (action === 'response_body') {
    if (!state.captureResponseBodies) {
      return { capture_enabled: false, item: null }
    }
    const pattern = request.params?.pattern ? String(request.params.pattern) : ''
    const item = pattern
      ? [...state.responseBodies].reverse().find((entry) => entry.url.includes(pattern))
      : state.responseBodies[state.responseBodies.length - 1]
    return { capture_enabled: true, item: item || null }
  }

  if (action === 'pdf') {
    const { page, targetId } = resolvePage(state, request)
    const outputPath = request.params?.path
      ? String(request.params.path)
      : path.join(paths.app_log_dir, `browser-${Date.now()}.pdf`)
    await ensureDir(path.dirname(outputPath))
    await page.pdf({ path: outputPath })
    return { target_id: targetId, path: outputPath }
  }

  if (action === 'cookies_get') {
    const urls = Array.isArray(request.params?.urls) ? request.params.urls : undefined
    return {
      cookies: await state.context.cookies(urls)
    }
  }

  if (action === 'cookies_set') {
    const cookie = request.params?.cookie
    const cookies = Array.isArray(request.params?.cookies)
      ? request.params.cookies
      : cookie
        ? [cookie]
        : []
    if (cookies.length === 0) throw new Error('cookies_set requires cookie or cookies')
    await state.context.addCookies(cookies)
    return { cookies_set: cookies.length }
  }

  if (action === 'cookies_clear') {
    await state.context.clearCookies()
    return { cookies_cleared: true }
  }

  if (action === 'storage_get') {
    const { page, targetId } = resolvePage(state, request)
    const kind = String(request.params?.kind || 'local')
    const key = request.params?.key ? String(request.params.key) : null
    const data = await page.evaluate(
      ({ kind, key }) => {
        const storage = kind === 'session' ? window.sessionStorage : window.localStorage
        if (key) {
          return { [key]: storage.getItem(key) }
        }
        const result = {}
        for (let i = 0; i < storage.length; i += 1) {
          const name = storage.key(i)
          if (name) result[name] = storage.getItem(name)
        }
        return result
      },
      { kind, key }
    )
    return { target_id: targetId, kind, data }
  }

  if (action === 'storage_set') {
    const { page, targetId } = resolvePage(state, request)
    const kind = String(request.params?.kind || 'local')
    const key = String(request.params?.key || '')
    const value = String(request.params?.value || '')
    if (!key) throw new Error('storage_set requires key')
    await page.evaluate(
      ({ kind, key, value }) => {
        const storage = kind === 'session' ? window.sessionStorage : window.localStorage
        storage.setItem(key, value)
      },
      { kind, key, value }
    )
    return { target_id: targetId, kind, key }
  }

  if (action === 'storage_clear') {
    const { page, targetId } = resolvePage(state, request)
    const kind = String(request.params?.kind || 'local')
    await page.evaluate((kind) => {
      const storage = kind === 'session' ? window.sessionStorage : window.localStorage
      storage.clear()
    }, kind)
    return { target_id: targetId, kind, cleared: true }
  }

  if (action === 'set_offline') {
    const enabled = Boolean(request.params?.enabled)
    await state.context.setOffline(enabled)
    return { offline: enabled }
  }

  if (action === 'set_headers') {
    const headers = request.params?.headers
    if (headers && typeof headers === 'object') {
      state.headers = headers
      await state.context.setExtraHTTPHeaders(headers)
      return { headers_set: true }
    }
    state.headers = undefined
    await state.context.setExtraHTTPHeaders({})
    return { headers_set: false }
  }

  if (action === 'set_credentials') {
    if (request.params?.clear) {
      state.credentials = undefined
      await closeProfile(profileName)
      await ensureContext(profileName, profileConfig, browserConfig, paths)
      return { credentials: 'cleared', restarted: true }
    }
    state.credentials = {
      username: String(request.params?.username || ''),
      password: String(request.params?.password || '')
    }
    await closeProfile(profileName)
    await ensureContext(profileName, profileConfig, browserConfig, paths)
    return { credentials: 'set', restarted: true }
  }

  if (action === 'set_geolocation') {
    if (request.params?.clear) {
      state.geolocation = undefined
      await closeProfile(profileName)
      await ensureContext(profileName, profileConfig, browserConfig, paths)
      return { geolocation: 'cleared', restarted: true }
    }
    state.geolocation = {
      latitude: Number(request.params?.latitude),
      longitude: Number(request.params?.longitude),
      accuracy: Number(request.params?.accuracy || 50)
    }
    await state.context.setGeolocation(state.geolocation)
    await state.context.grantPermissions(['geolocation']).catch(() => undefined)
    return { geolocation: state.geolocation }
  }

  if (action === 'set_media') {
    const scheme = String(request.params?.scheme || request.params?.color_scheme || 'light')
    state.media = scheme
    for (const [, page] of state.pages) {
      await page.emulateMedia({ colorScheme: scheme }).catch(() => undefined)
    }
    return { media: scheme }
  }

  if (action === 'trace_start') {
    if (state.traceStarted) {
      return { trace_started: true, trace_path: state.tracePath }
    }
    await state.context.tracing.start({ screenshots: true, snapshots: true, sources: true })
    state.traceStarted = true
    return { trace_started: true }
  }

  if (action === 'trace_stop') {
    if (!state.traceStarted) {
      return { trace_started: false }
    }
    const tracePath = request.params?.path
      ? String(request.params.path)
      : path.join(paths.app_log_dir, `browser-trace-${Date.now()}.zip`)
    await ensureDir(path.dirname(tracePath))
    await state.context.tracing.stop({ path: tracePath })
    state.traceStarted = false
    state.tracePath = tracePath
    return { trace_started: false, trace_path: tracePath }
  }

  if (action === 'evaluate') {
    if (!browserConfig.evaluate_enabled) {
      throw new Error('evaluate is disabled by policy (browser.evaluate_enabled=false)')
    }
    const { page, targetId } = resolvePage(state, request)
    const expression = String(request.params?.expression || request.params?.script || '')
    if (!expression) throw new Error('evaluate requires params.expression')
    const result = await page.evaluate(expression)
    return { target_id: targetId, result }
  }

  throw new Error(`Unsupported browser action: ${action}`)
}

async function dispatch(requestLine) {
  let request
  try {
    request = JSON.parse(requestLine)
  } catch (error) {
    return {
      id: null,
      ok: false,
      data: null,
      error: `Invalid request JSON: ${makeErrorMessage(error)}`,
      meta: { ts: nowIso() }
    }
  }

  const started = Date.now()
  const id = request?.id ?? null
  try {
    if (request.method === 'health') {
      const durationMs = Date.now() - started
      return {
        id,
        ok: true,
        data: { status: 'ok', ts: nowIso() },
        error: null,
        meta: {
          duration_ms: durationMs,
          resolve_ms: 0,
          locate_ms: 0,
          action_ms: 0,
          fallback_count: 0,
          total_ms: durationMs
        }
      }
    }
    if (request.method === 'shutdown') {
      for (const [name] of runtime.profiles) {
        await closeProfile(name)
      }
      const durationMs = Date.now() - started
      return {
        id,
        ok: true,
        data: { shutdown: true },
        error: null,
        meta: {
          duration_ms: durationMs,
          resolve_ms: 0,
          locate_ms: 0,
          action_ms: 0,
          fallback_count: 0,
          total_ms: durationMs
        }
      }
    }
    if (request.method !== 'browser.action') {
      throw new Error(`Unknown method: ${request.method}`)
    }
    const rawData = await handleAction(request.params)
    const { payload, perf } = movePerfFromData(rawData)
    const durationMs = Date.now() - started
    const perfMeta = buildPerfMeta({
      ...(perf || {}),
      duration_ms: (perf && Number.isFinite(Number(perf.duration_ms)))
        ? Number(perf.duration_ms)
        : durationMs
    })
    return {
      id,
      ok: true,
      data: payload,
      error: null,
      meta: {
        duration_ms: durationMs,
        ...perfMeta
      }
    }
  } catch (error) {
    const durationMs = Date.now() - started
    return {
      id,
      ok: false,
      data: null,
      error: makeErrorMessage(error),
      meta: {
        duration_ms: durationMs,
        resolve_ms: 0,
        locate_ms: 0,
        action_ms: 0,
        fallback_count: 0,
        total_ms: durationMs
      }
    }
  }
}

async function main() {
  const rl = readline.createInterface({
    input: process.stdin,
    crlfDelay: Infinity
  })

  for await (const line of rl) {
    if (!line || !line.trim()) continue
    const response = await dispatch(line)
    process.stdout.write(`${JSON.stringify(response)}\n`)
  }
}

main().catch((error) => {
  process.stderr.write(`[browser-sidecar] fatal: ${makeErrorMessage(error)}\n`)
  process.exit(1)
})
