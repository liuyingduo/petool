import fs from 'node:fs/promises'
import path from 'node:path'
import readline from 'node:readline'
import process from 'node:process'
import { chromium, devices } from 'playwright'

const runtime = {
  profiles: new Map()
}

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
      headers: undefined,
      credentials: undefined,
      geolocation: undefined,
      media: undefined,
      timezone: undefined,
      locale: undefined,
      device: undefined,
      traceStarted: false,
      tracePath: null
    }
    runtime.profiles.set(profile, state)
  }
  return state
}

function clearProfilePages(state) {
  state.pages.clear()
  state.pageIds.clear()
  state.refsByTarget.clear()
  state.activeTargetId = null
}

async function attachExistingBrowserViaCdp(profileConfig, state) {
  const cdpUrl = typeof profileConfig?.cdp_url === 'string' ? profileConfig.cdp_url.trim() : ''
  if (!cdpUrl) {
    throw new Error('cdp_url is required for attach mode')
  }
  const browser = await chromium.connectOverCDP(cdpUrl)
  const contexts = browser.contexts()
  const context = contexts[0] || null
  if (!context) {
    throw new Error(`No context found after connecting to CDP endpoint: ${cdpUrl}`)
  }
  state.browser = browser
  state.context = context
  state.connectionMode = 'cdp'
  clearProfilePages(state)
  for (const page of context.pages()) {
    registerPage(state, page)
  }
  context.on('page', (page) => {
    registerPage(state, page)
  })
}

function attachPageListeners(state, page, targetId) {
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
    pushLimited(state.requests, {
      url: request.url(),
      method: request.method(),
      resource_type: request.resourceType(),
      target_id: targetId,
      ts: nowIso()
    })
  })

  page.on('response', async (response) => {
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

  page.on('close', () => {
    state.pages.delete(targetId)
    state.pageIds.delete(page)
    state.refsByTarget.delete(targetId)
    if (state.activeTargetId === targetId) {
      const next = Array.from(state.pages.keys())[0] || null
      state.activeTargetId = next
    }
  })
}

function registerPage(state, page) {
  const existing = state.pageIds.get(page)
  if (existing) return existing
  const targetId = `t${state.nextTargetSeq++}`
  state.pages.set(targetId, page)
  state.pageIds.set(page, targetId)
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

async function launchContext(profileName, profileConfig, browserConfig, paths, state) {
  const userDataDir = (typeof profileConfig?.user_data_dir === 'string' && profileConfig.user_data_dir.trim())
    ? profileConfig.user_data_dir.trim()
    : profileUserDataDir(paths, profileName)
  await ensureDir(userDataDir)

  const viewport = defaultViewport(profileConfig)
  const launchOptions = {
    headless: Boolean(profileConfig?.headless),
    viewport
  }

  const executablePath = typeof profileConfig?.executable_path === 'string'
    ? profileConfig.executable_path.trim()
    : ''
  if (executablePath) {
    launchOptions.executablePath = executablePath
  }
  if (profileConfig?.engine === 'chrome' && !profileConfig?.executable_path) {
    launchOptions.channel = 'chrome'
  }
  if (state.headers && typeof state.headers === 'object') {
    launchOptions.extraHTTPHeaders = state.headers
  }
  if (state.credentials) {
    launchOptions.httpCredentials = state.credentials
  }
  if (state.geolocation) {
    launchOptions.geolocation = state.geolocation
    launchOptions.permissions = ['geolocation']
  }
  if (state.locale) {
    launchOptions.locale = state.locale
  }
  if (state.timezone) {
    launchOptions.timezoneId = state.timezone
  }
  if (state.device && devices[state.device]) {
    Object.assign(launchOptions, devices[state.device])
  }

  const context = await chromium.launchPersistentContext(userDataDir, launchOptions)
  state.browser = null
  state.context = context
  state.connectionMode = 'persistent'
  clearProfilePages(state)
  for (const page of context.pages()) {
    registerPage(state, page)
  }
  context.on('page', (page) => {
    registerPage(state, page)
  })

  if (state.media) {
    for (const page of context.pages()) {
      await page.emulateMedia({ colorScheme: state.media }).catch(() => undefined)
    }
  }

  if (browserConfig.allow_private_network === false) {
    context.route('**', async (route) => {
      try {
        const url = route.request().url()
        assertPrivateNetworkAllowed(url, browserConfig.allow_private_network)
      } catch (error) {
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

async function ensureContext(profileName, profileConfig, browserConfig, paths) {
  const state = getProfileState(profileName)
  if (!state.context) {
    const cdpUrl = typeof profileConfig?.cdp_url === 'string' ? profileConfig.cdp_url.trim() : ''
    if (cdpUrl) {
      await attachExistingBrowserViaCdp(profileConfig, state)
    } else {
      const executablePath = typeof profileConfig?.executable_path === 'string'
        ? profileConfig.executable_path.trim()
        : ''
      if (!executablePath) {
        throw new Error('Profile must provide executable_path or cdp_url to control user browser')
      }
      await launchContext(profileName, profileConfig, browserConfig, paths, state)
    }
    return state
  }
  return state
}

async function closeProfile(profileName) {
  const state = runtime.profiles.get(profileName)
  if (!state || !state.context) return
  if (state.connectionMode === 'cdp') {
    if (state.browser) {
      await state.browser.close().catch(() => undefined)
    } else {
      await state.context.close().catch(() => undefined)
    }
  } else {
    await state.context.close().catch(() => undefined)
  }
  state.browser = null
  state.context = null
  state.connectionMode = null
  clearProfilePages(state)
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

function normalizeSnapshotRows(rows) {
  const result = []
  let index = 1
  for (const row of rows) {
    result.push({
      ref: `e${index++}`,
      role: row.role || row.tag || 'element',
      text: row.text || '',
      selector: row.selector
    })
  }
  return result
}

async function buildSnapshot(state, request) {
  const { page, targetId } = resolvePage(state, request)
  const snapshotRows = await page.evaluate(() => {
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

    function isVisible(el) {
      const style = getComputedStyle(el)
      if (style.display === 'none' || style.visibility === 'hidden') return false
      const rect = el.getBoundingClientRect()
      return rect.width > 0 && rect.height > 0
    }

    const candidates = Array.from(
      document.querySelectorAll('a,button,input,textarea,select,[role],summary,[tabindex],[onclick]')
    )
      .filter((el) => isVisible(el))
      .slice(0, 300)

    return candidates.map((el) => ({
      selector: cssPath(el),
      role: el.getAttribute('role') || el.tagName.toLowerCase(),
      tag: el.tagName.toLowerCase(),
      text: (el.innerText || el.textContent || '').trim().slice(0, 160)
    }))
  })

  const refs = normalizeSnapshotRows(snapshotRows)
  const refMap = new Map(refs.map((row) => [row.ref, row.selector]))
  state.refsByTarget.set(targetId, refMap)

  return {
    target_id: targetId,
    url: page.url(),
    title: await page.title().catch(() => ''),
    refs,
    stats: {
      count: refs.length
    }
  }
}

function resolveSelectorFromRef(state, targetId, ref) {
  const map = state.refsByTarget.get(targetId)
  if (!map) return null
  return map.get(ref) || null
}

async function executeAct(state, request) {
  const params = requireObject(request.params || {}, 'params')
  const { page, targetId } = resolvePage(state, request)
  const kind = typeof params.kind === 'string' ? params.kind : ''
  if (!kind) throw new Error('act.kind is required')

  const ref = typeof params.ref === 'string' ? params.ref : null
  const selector = typeof params.selector === 'string'
    ? params.selector
    : (ref ? resolveSelectorFromRef(state, targetId, ref) : null)

  switch (kind) {
    case 'click':
      if (!selector) throw new Error('act.click requires ref or selector')
      await page.click(selector, {
        button: typeof params.button === 'string' ? params.button : 'left',
        clickCount: params.double ? 2 : 1,
        timeout: Number.isInteger(params.timeout_ms) ? params.timeout_ms : undefined
      })
      return { ok: true, action: 'click', target_id: targetId, selector }
    case 'type':
      if (!selector) throw new Error('act.type requires ref or selector')
      if (params.replace !== false) {
        await page.fill(selector, String(params.text || ''))
      } else {
        await page.type(selector, String(params.text || ''))
      }
      if (params.submit) {
        await page.keyboard.press('Enter')
      }
      return { ok: true, action: 'type', target_id: targetId, selector }
    case 'press':
      await page.keyboard.press(String(params.key || 'Enter'))
      return { ok: true, action: 'press', target_id: targetId }
    case 'hover':
      if (!selector) throw new Error('act.hover requires ref or selector')
      await page.hover(selector)
      return { ok: true, action: 'hover', target_id: targetId, selector }
    case 'scroll':
      await page.evaluate(
        ({ x, y }) => window.scrollBy(Number(x) || 0, Number(y) || 0),
        { x: params.x, y: params.y }
      )
      return { ok: true, action: 'scroll', target_id: targetId }
    case 'select':
      if (!selector) throw new Error('act.select requires ref or selector')
      await page.selectOption(selector, params.values || params.value || [])
      return { ok: true, action: 'select', target_id: targetId, selector }
    case 'wait':
      if (params.selector) {
        await page.waitForSelector(String(params.selector), {
          timeout: Number.isInteger(params.timeout_ms) ? params.timeout_ms : undefined
        })
      } else if (params.url) {
        await page.waitForURL(String(params.url), {
          timeout: Number.isInteger(params.timeout_ms) ? params.timeout_ms : undefined
        })
      } else {
        await page.waitForTimeout(Number.isInteger(params.timeout_ms) ? params.timeout_ms : 1000)
      }
      return { ok: true, action: 'wait', target_id: targetId }
    case 'drag': {
      const fromRef = typeof params.from_ref === 'string' ? params.from_ref : null
      const toRef = typeof params.to_ref === 'string' ? params.to_ref : null
      const fromSelector = fromRef ? resolveSelectorFromRef(state, targetId, fromRef) : null
      const toSelector = toRef ? resolveSelectorFromRef(state, targetId, toRef) : null
      if (!fromSelector || !toSelector) throw new Error('act.drag requires from_ref and to_ref')
      await page.dragAndDrop(fromSelector, toSelector)
      return { ok: true, action: 'drag', target_id: targetId }
    }
    default:
      throw new Error(`Unsupported act.kind: ${kind}`)
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
        running: Boolean(runtime.profiles.get(name)?.context),
        mode: runtime.profiles.get(name)?.connectionMode || null
      })),
      default_profile: browserConfig.default_profile
    }
  }

  if (action === 'status') {
    const state = getProfileState(profileName)
    return {
      profile: profileName,
      running: Boolean(state.context),
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

  if (action === 'start') {
    const hasCdp = typeof profileConfig?.cdp_url === 'string' && profileConfig.cdp_url.trim().length > 0
    const hasExecutable = typeof profileConfig?.executable_path === 'string' && profileConfig.executable_path.trim().length > 0
    if (!hasCdp && !hasExecutable) {
      throw new Error(
        `Profile "${profileName}" must set cdp_url (attach to existing browser) or executable_path (launch user browser).`
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
    const html = await page.content()
    const links = extractPageLinks(html, page.url(), Number(request.params?.max_links) || 30)
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
    return await executeAct(state, request)
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
    const pattern = request.params?.pattern ? String(request.params.pattern) : ''
    const item = pattern
      ? [...state.responseBodies].reverse().find((entry) => entry.url.includes(pattern))
      : state.responseBodies[state.responseBodies.length - 1]
    return { item: item || null }
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
      return {
        id,
        ok: true,
        data: { status: 'ok', ts: nowIso() },
        error: null,
        meta: { duration_ms: Date.now() - started }
      }
    }
    if (request.method === 'shutdown') {
      for (const [name] of runtime.profiles) {
        await closeProfile(name)
      }
      return {
        id,
        ok: true,
        data: { shutdown: true },
        error: null,
        meta: { duration_ms: Date.now() - started }
      }
    }
    if (request.method !== 'browser.action') {
      throw new Error(`Unknown method: ${request.method}`)
    }
    const data = await handleAction(request.params)
    return {
      id,
      ok: true,
      data,
      error: null,
      meta: { duration_ms: Date.now() - started }
    }
  } catch (error) {
    return {
      id,
      ok: false,
      data: null,
      error: makeErrorMessage(error),
      meta: { duration_ms: Date.now() - started }
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
