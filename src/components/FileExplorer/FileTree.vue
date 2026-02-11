<template>
  <div class="file-tree">
    <div
      v-for="file in files"
      :key="file.path"
      class="file-item"
      :style="{ paddingLeft: `${level * 16 + 8}px` }"
    >
      <div
        class="file-row"
        @click="handleClick(file)"
        @dblclick="handleDoubleClick(file)"
      >
        <el-icon
          v-if="file.is_dir"
          class="expand-icon"
          :class="{ expanded: isExpanded(file.path) }"
        >
          <ArrowRight />
        </el-icon>
        <span v-else class="icon-placeholder"></span>

        <el-icon class="file-icon" :color="getFileIconColor(file)">
          <component :is="getFileIcon(file)" />
        </el-icon>

        <span class="file-name text-ellipsis">{{ file.name }}</span>
      </div>

      <!-- Expanded children -->
      <FileTree
        v-if="file.is_dir && isExpanded(file.path)"
        :files="getChildren(file.path)"
        :level="level + 1"
        @file-click="$emit('file-click', $event)"
      />
    </div>
  </div>
</template>

<script setup lang="ts">
import { ArrowRight, Folder, FolderOpened, Document } from '@element-plus/icons-vue'
import { useFilesystemStore } from '@/stores/filesystem'

interface FileInfo {
  name: string
  path: string
  is_dir: boolean
  size?: number
  extension?: string
}

interface Props {
  files: FileInfo[]
  level?: number
}

interface Emits {
  (e: 'file-click', file: FileInfo): void
}

withDefaults(defineProps<Props>(), {
  level: 0
})
const emit = defineEmits<Emits>()

const fsStore = useFilesystemStore()

function isExpanded(path: string) {
  return fsStore.isExpanded(path)
}

async function handleClick(file: FileInfo) {
  if (file.is_dir) {
    const wasExpanded = fsStore.isExpanded(file.path)
    fsStore.toggleExpanded(file.path)
    if (!wasExpanded) {
      await fsStore.loadChildren(file.path)
    }
  }
}

function handleDoubleClick(file: FileInfo) {
  if (!file.is_dir) {
    emit('file-click', file)
  }
}

function getChildren(path: string) {
  return fsStore.getChildren(path)
}

function getFileIcon(file: FileInfo) {
  if (file.is_dir) {
    return isExpanded(file.path) ? FolderOpened : Folder
  }
  return Document
}

function getFileIconColor(file: FileInfo) {
  if (file.is_dir) return '#f7ba2e'
  return '#909399'
}
</script>

<style scoped>
.file-tree {
  font-size: 13px;
}

.file-item {
  user-select: none;
}

.file-row {
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 4px 8px;
  border-radius: 4px;
  cursor: pointer;
  transition: background-color 0.15s;
}

.file-row:hover {
  background-color: var(--color-surface-hover);
}

.expand-icon {
  transition: transform 0.2s;
  font-size: 12px;
}

.expand-icon.expanded {
  transform: rotate(90deg);
}

.icon-placeholder {
  width: 12px;
}

.file-icon {
  font-size: 16px;
}

.file-name {
  flex: 1;
  color: var(--color-text);
}
</style>
