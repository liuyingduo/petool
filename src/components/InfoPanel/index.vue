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

          <div v-else class="file-tree">
            <FileTree :files="fsStore.files" />
          </div>
        </div>
      </el-tab-pane>

      <!-- Skills Tab -->
      <el-tab-pane label="Skills" name="skills">
        <div class="tab-content">
          <div class="empty-state">
            <el-icon :size="48"><Box /></el-icon>
            <p>No skills installed</p>
            <el-button size="small">Browse Skills</el-button>
          </div>
        </div>
      </el-tab-pane>
    </el-tabs>

    <SettingsDialog v-model="showSettings" />
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { useConfigStore } from '@/stores/config'
import { useFilesystemStore } from '@/stores/filesystem'
import SettingsDialog from '../Settings/index.vue'
import FileTree from '../FileExplorer/FileTree.vue'
import { Setting, FolderOpened, Box } from '@element-plus/icons-vue'

const configStore = useConfigStore()
const fsStore = useFilesystemStore()
const activeTab = ref('settings')
const showSettings = ref(false)

async function handleSelectFolder() {
  await fsStore.selectFolder()
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

.file-tree {
  min-height: 100%;
}
</style>
