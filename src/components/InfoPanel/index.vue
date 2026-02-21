<template>
  <div class="info-panel-container">
    <!-- Tabs -->
    <el-tabs v-model="activeTab" class="info-tabs">
      <!-- Settings Tab -->
      <el-tab-pane label="Settings" name="settings">
        <div class="tab-content">
          <div class="info-section">
            <h4>API Configuration</h4>
            <div class="info-item">
              <span class="label">Model:</span>
              <span class="value">{{ configStore.config.model || 'Not set' }}</span>
            </div>
            <div class="info-item">
              <span class="label">API Base:</span>
              <span class="value text-ellipsis">{{ configStore.config.api_base || 'Default' }}</span>
            </div>
            <div class="info-item">
              <span class="label">API Key:</span>
              <span class="value">{{ configStore.config.api_key ? '••••••••' : 'Not set' }}</span>
            </div>
          </div>

          <div class="info-section" v-if="configStore.config.work_directory">
            <h4>Work Directory</h4>
            <div class="info-item">
              <span class="value text-ellipsis">{{ configStore.config.work_directory }}</span>
            </div>
          </div>

          <el-button @click="showSettings = true" style="width: 100%">
            <el-icon><Setting /></el-icon>
            Open Settings
          </el-button>
        </div>
      </el-tab-pane>

      <!-- Files Tab -->
      <el-tab-pane label="Files" name="files">
        <div class="tab-content">
          <div v-if="!fsStore.currentDirectory" class="empty-state">
            <el-button @click="handleSelectFolder">
              <el-icon><FolderOpened /></el-icon>
              Select Folder
            </el-button>
          </div>

          <div v-else class="files-layout">
            <div class="file-tree-pane">
              <FileTree :files="fsStore.files" @file-click="handleFileClick" />
            </div>
            <div class="file-editor-pane">
              <div v-if="!selectedFilePath" class="empty-state">
                <p>Double-click a file to preview and edit</p>
              </div>
              <template v-else>
                <div class="editor-header">
                  <div class="text-ellipsis">{{ selectedFilePath }}</div>
                  <el-button
                    size="small"
                    type="primary"
                    :disabled="!isFileDirty || savingFile"
                    :loading="savingFile"
                    @click="handleSaveFile"
                  >
                    Save
                  </el-button>
                </div>
                <el-input
                  v-model="selectedFileContent"
                  type="textarea"
                  :rows="24"
                  resize="none"
                  class="file-editor-textarea"
                />
              </template>
            </div>
          </div>
        </div>
      </el-tab-pane>

      <!-- Skills Tab -->
      <el-tab-pane label="Skills" name="skills">
        <div class="tab-content">
          <SkillsManager />
        </div>
      </el-tab-pane>
    </el-tabs>

    <SettingsDialog v-model="showSettings" />
  </div>
</template>

<script setup lang="ts">
import { computed, ref } from 'vue'
import { useConfigStore } from '@/stores/config'
import { useFilesystemStore } from '@/stores/filesystem'
import SettingsDialog from '../Settings/index.vue'
import FileTree from '../FileExplorer/FileTree.vue'
import SkillsManager from '../SkillsManager/index.vue'
import { Setting, FolderOpened } from '@element-plus/icons-vue'
import { ElMessage } from 'element-plus'

const configStore = useConfigStore()
const fsStore = useFilesystemStore()
const activeTab = ref('settings')
const showSettings = ref(false)
const selectedFilePath = ref('')
const selectedFileContent = ref('')
const originalFileContent = ref('')
const savingFile = ref(false)

const isFileDirty = computed(() => selectedFileContent.value !== originalFileContent.value)

async function handleSelectFolder() {
  await fsStore.selectFolder()
  selectedFilePath.value = ''
  selectedFileContent.value = ''
  originalFileContent.value = ''
}

async function handleFileClick(file: { path: string; is_dir: boolean }) {
  if (file.is_dir) return
  try {
    const content = await fsStore.readFile(file.path)
    selectedFilePath.value = file.path
    selectedFileContent.value = content
    originalFileContent.value = content
  } catch (error) {
    ElMessage.error('Failed to open file')
  }
}

async function handleSaveFile() {
  if (!selectedFilePath.value || !isFileDirty.value) return

  savingFile.value = true
  try {
    await fsStore.writeFile(selectedFilePath.value, selectedFileContent.value)
    originalFileContent.value = selectedFileContent.value
    ElMessage.success('File saved')
  } catch (error) {
    ElMessage.error('Failed to save file')
  } finally {
    savingFile.value = false
  }
}
</script>

<style scoped>
.info-panel-container {
  display: flex;
  flex-direction: column;
  height: 100%;
  background-color: var(--color-surface);
}

.info-tabs {
  flex: 1;
  display: flex;
  flex-direction: column;
}

.info-tabs :deep(.el-tabs__header) {
  margin: 0;
  background-color: var(--color-surface);
  border-bottom: 1px solid var(--color-border);
}

.info-tabs :deep(.el-tabs__nav-wrap) {
  padding: 0 16px;
}

.info-tabs :deep(.el-tabs__content) {
  flex: 1;
  overflow: hidden;
}

.info-tabs :deep(.el-tab-pane) {
  height: 100%;
}

.tab-content {
  height: 100%;
  padding: 16px;
  overflow-y: auto;
}

.info-section {
  margin-bottom: 20px;
}

.info-section h4 {
  margin: 0 0 12px 0;
  font-size: 12px;
  font-weight: 500;
  text-transform: uppercase;
  color: var(--color-text-secondary);
  letter-spacing: 0.5px;
}

.info-item {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 8px 0;
  border-bottom: 1px solid var(--color-border);
}

.info-item:last-child {
  border-bottom: none;
}

.info-item .label {
  font-size: 13px;
  color: var(--color-text-secondary);
}

.info-item .value {
  font-size: 13px;
  color: var(--color-text);
  max-width: 60%;
  text-align: right;
}

.empty-state {
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  height: 100%;
  gap: 16px;
  color: var(--color-text-secondary);
}

.empty-state p {
  margin: 0;
}

.files-layout {
  min-height: 100%;
  height: 100%;
  display: grid;
  grid-template-columns: 240px 1fr;
  gap: 12px;
}

.file-tree-pane {
  border: 1px solid var(--color-border);
  border-radius: 12px;
  padding: 12px;
  overflow: auto;
  background: var(--color-surface);
}

.file-editor-pane {
  border: 1px solid var(--color-border);
  border-radius: 12px;
  overflow: hidden;
  display: flex;
  flex-direction: column;
  background: var(--color-surface);
}

.editor-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
  padding: 8px 10px;
  border-bottom: 1px solid var(--color-border);
}

.file-editor-textarea {
  flex: 1;
}

.file-editor-textarea :deep(.el-textarea__inner) {
  height: 100%;
  border: none;
  border-radius: 0;
  background: var(--color-bg);
  color: var(--color-text);
  font-family: Consolas, Monaco, monospace;
}
</style>
