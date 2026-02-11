import { defineStore } from 'pinia'
import { ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'

export interface FileInfo {
  name: string
  path: string
  is_dir: boolean
  size?: number
  extension?: string
}

export const useFilesystemStore = defineStore('filesystem', () => {
  const currentDirectory = ref<string | null>(null)
  const files = ref<FileInfo[]>([])
  const loading = ref(false)
  const expandedPaths = ref<Set<string>>(new Set())

  async function selectFolder() {
    try {
      const path = await invoke<string | null>('select_folder')
      if (path) {
        currentDirectory.value = path
        await scanDirectory(path)
      }
    } catch (error) {
      console.error('Failed to select folder:', error)
      throw error
    }
  }

  async function scanDirectory(path: string) {
    loading.value = true
    try {
      files.value = await invoke<FileInfo[]>('scan_directory', { path })
    } catch (error) {
      console.error('Failed to scan directory:', error)
    } finally {
      loading.value = false
    }
  }

  async function readFile(path: string) {
    try {
      return await invoke<string>('read_file', { path })
    } catch (error) {
      console.error('Failed to read file:', error)
      throw error
    }
  }

  function toggleExpanded(path: string) {
    if (expandedPaths.value.has(path)) {
      expandedPaths.value.delete(path)
    } else {
      expandedPaths.value.add(path)
    }
  }

  function isExpanded(path: string) {
    return expandedPaths.value.has(path)
  }

  return {
    currentDirectory,
    files,
    loading,
    expandedPaths,
    selectFolder,
    scanDirectory,
    readFile,
    toggleExpanded,
    isExpanded
  }
})
