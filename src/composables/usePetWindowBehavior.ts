import { cursorPosition, getCurrentWindow } from '@tauri-apps/api/window'
import type { Ref } from 'vue'

const DRAG_EXCLUDED_SELECTOR =
  'button, input, textarea, select, a, label, [contenteditable="true"], .create-dialog, .message-list, .bubble, .reasoning'

const DEFAULT_SHAPE = {
  edgeFeather: 2,
  bodyRadius: 34
}

export function usePetWindowBehavior(workspaceRef: Ref<HTMLElement | null>) {
  const appWindow = getCurrentWindow()
  let cursorPassthroughActive = false
  let cursorPassthroughProbeTimer: number | null = null
  let pointerTrackingEnabled = false

  function handleManualDrag() {
    void appWindow.startDragging().catch(() => {
      // no-op fallback when the platform/region doesn't support manual dragging
    })
  }

  function handleWorkspaceMouseDown(event: MouseEvent) {
    const target = event.target
    if (!(target instanceof HTMLElement)) return
    if (isDragExcludedTarget(target)) return
    handleManualDrag()
  }

  function setupCursorPassthrough() {
    if (typeof window === 'undefined') return
    const runtimeWindow = window as Window & { __TAURI_INTERNALS__?: unknown }
    if (!runtimeWindow.__TAURI_INTERNALS__) return
    if (!/Windows/i.test(navigator.userAgent)) return
    if (pointerTrackingEnabled) return

    pointerTrackingEnabled = true
    window.addEventListener('mousemove', handlePointerMove, { passive: true })
    window.addEventListener('mousedown', handlePointerMove, { passive: true })
  }

  function teardownCursorPassthrough() {
    if (typeof window !== 'undefined' && pointerTrackingEnabled) {
      window.removeEventListener('mousemove', handlePointerMove)
      window.removeEventListener('mousedown', handlePointerMove)
    }
    pointerTrackingEnabled = false
    stopCursorRecoveryProbe()

    if (cursorPassthroughActive) {
      void appWindow.setIgnoreCursorEvents(false).catch(() => {
        // no-op fallback on unsupported platforms
      })
      cursorPassthroughActive = false
    }
  }

  function isDragExcludedTarget(target: HTMLElement) {
    return Boolean(target.closest(DRAG_EXCLUDED_SELECTOR))
  }

  function handlePointerMove(event: MouseEvent) {
    if (cursorPassthroughActive) return
    void updateCursorPassthrough(event.clientX, event.clientY)
  }

  async function updateCursorPassthrough(clientX: number, clientY: number) {
    const shouldIgnore = !isPointInsidePetShape(clientX, clientY)
    if (shouldIgnore === cursorPassthroughActive) return

    try {
      await appWindow.setIgnoreCursorEvents(shouldIgnore)
      cursorPassthroughActive = shouldIgnore
      if (shouldIgnore) {
        startCursorRecoveryProbe()
      } else {
        stopCursorRecoveryProbe()
      }
    } catch {
      stopCursorRecoveryProbe()
      cursorPassthroughActive = false
    }
  }

  function startCursorRecoveryProbe() {
    if (cursorPassthroughProbeTimer !== null) return
    cursorPassthroughProbeTimer = window.setInterval(() => {
      void recoverCursorEvents()
    }, 120)
  }

  function stopCursorRecoveryProbe() {
    if (cursorPassthroughProbeTimer === null) return
    window.clearInterval(cursorPassthroughProbeTimer)
    cursorPassthroughProbeTimer = null
  }

  async function recoverCursorEvents() {
    if (!cursorPassthroughActive) {
      stopCursorRecoveryProbe()
      return
    }

    try {
      const [cursor, winPos, winSize] = await Promise.all([
        cursorPosition(),
        appWindow.outerPosition(),
        appWindow.outerSize()
      ])

      const insideWindowBounds =
        cursor.x >= winPos.x &&
        cursor.y >= winPos.y &&
        cursor.x <= winPos.x + winSize.width &&
        cursor.y <= winPos.y + winSize.height

      if (!insideWindowBounds) return

      await appWindow.setIgnoreCursorEvents(false)
      cursorPassthroughActive = false
      stopCursorRecoveryProbe()
    } catch {
      stopCursorRecoveryProbe()
      cursorPassthroughActive = false
    }
  }

  function isPointInsidePetShape(clientX: number, clientY: number) {
    const workspace = workspaceRef.value
    if (!workspace) return true

    const rect = workspace.getBoundingClientRect()
    return isPointInsideRoundedRect(
      clientX,
      clientY,
      rect.left - DEFAULT_SHAPE.edgeFeather,
      rect.top - DEFAULT_SHAPE.edgeFeather,
      rect.width + DEFAULT_SHAPE.edgeFeather * 2,
      rect.height + DEFAULT_SHAPE.edgeFeather * 2,
      DEFAULT_SHAPE.bodyRadius + DEFAULT_SHAPE.edgeFeather
    )
  }

  function isPointInsideRoundedRect(
    px: number,
    py: number,
    left: number,
    top: number,
    width: number,
    height: number,
    radius: number
  ) {
    const right = left + width
    const bottom = top + height
    if (px < left || px > right || py < top || py > bottom) return false

    const safeRadius = Math.max(0, Math.min(radius, width / 2, height / 2))
    const nearestX = Math.max(left + safeRadius, Math.min(px, right - safeRadius))
    const nearestY = Math.max(top + safeRadius, Math.min(py, bottom - safeRadius))
    const dx = px - nearestX
    const dy = py - nearestY
    return dx * dx + dy * dy <= safeRadius * safeRadius
  }

  return {
    handleManualDrag,
    handleWorkspaceMouseDown,
    setupCursorPassthrough,
    teardownCursorPassthrough
  }
}
