import { ref } from 'vue'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { invoke } from '@tauri-apps/api/core'

export function useWindowControls() {
  const appWindow = getCurrentWindow()
  const isWindowMaximized = ref(false)

  async function syncWindowMaximizedState() {
    try {
      isWindowMaximized.value = await appWindow.isMaximized()
    } catch {
      isWindowMaximized.value = false
    }
  }

  async function handleMinimize() {
    try {
      await appWindow.minimize()
    } catch {
      // ignore
    }
  }

  async function handleToggleMaximize() {
    try {
      await appWindow.toggleMaximize()
      await syncWindowMaximizedState()
    } catch {
      // ignore
    }
  }

  async function handleClose() {
    try {
      await invoke('app_exit_now')
    } catch {
      // ignore
    }
  }

  void syncWindowMaximizedState()

  return {
    isWindowMaximized,
    handleMinimize,
    handleToggleMaximize,
    handleClose,
    syncWindowMaximizedState
  }
}
