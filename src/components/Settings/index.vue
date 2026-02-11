<template>
  <el-dialog
    v-model="dialogVisible"
    title="Settings"
    width="600px"
    :close-on-click-modal="false"
  >
    <el-tabs v-model="activeTab">
      <!-- API Settings -->
      <el-tab-pane label="API Configuration" name="api">
        <el-form :model="localConfig" label-width="120px">
          <el-form-item label="API Key">
            <el-input
              v-model="localConfig.api_key"
              type="password"
              placeholder="Enter your API key"
              show-password
            />
          </el-form-item>

          <el-form-item label="API Base URL">
            <el-input
              v-model="localConfig.api_base"
              placeholder="https://open.bigmodel.cn/api/paas/v4"
            />
          </el-form-item>

          <el-form-item label="Model">
            <el-select v-model="localConfig.model" style="width: 100%">
              <el-option label="GLM-4.7" value="glm-4.7" />
              <el-option label="GLM-4-Flash" value="glm-4-flash" />
              <el-option label="GLM-4-Air" value="glm-4-air" />
              <el-option label="GLM-4" value="glm-4" />
            </el-select>
          </el-form-item>

          <el-form-item label="System Prompt">
            <el-input
              v-model="localConfig.system_prompt"
              type="textarea"
              :rows="4"
              placeholder="Optional system prompt for assistant behavior"
            />
          </el-form-item>

          <el-form-item label="Work Directory">
            <el-input
              v-model="localConfig.work_directory"
              placeholder="Select your project directory"
              readonly
            >
              <template #append>
                <el-button @click="handleSelectFolder">
                  <el-icon><Folder /></el-icon>
                </el-button>
              </template>
            </el-input>
          </el-form-item>

          <el-form-item>
            <el-button
              type="primary"
              @click="handleValidate"
              :loading="validating"
            >
              Validate API Key
            </el-button>
          </el-form-item>
        </el-form>
      </el-tab-pane>

      <!-- Appearance -->
      <el-tab-pane label="Appearance" name="appearance">
        <el-form label-width="120px">
          <el-form-item label="Theme">
            <el-radio-group v-model="localConfig.theme">
              <el-radio value="dark">Dark</el-radio>
              <el-radio value="light">Light</el-radio>
            </el-radio-group>
          </el-form-item>
        </el-form>
      </el-tab-pane>

      <!-- MCP Servers -->
      <el-tab-pane label="MCP Servers" name="mcp">
        <div class="mcp-list">
          <div
            v-for="(server, index) in localConfig.mcp_servers"
            :key="index"
            class="mcp-item"
          >
            <div class="mcp-info">
              <strong>{{ server.name }}</strong>
              <span class="mcp-type">{{ server.transport.type }}</span>
            </div>
            <div class="mcp-actions">
              <el-switch v-model="server.enabled" size="small" />
              <el-button
                type="danger"
                size="small"
                text
                @click="removeMcpServer(index)"
              >
                <el-icon><Delete /></el-icon>
              </el-button>
            </div>
          </div>

          <el-empty
            v-if="localConfig.mcp_servers.length === 0"
            description="No MCP servers configured"
            :image-size="60"
          />

          <el-button @click="showAddMcpDialog = true" style="width: 100%; margin-top: 12px">
            <el-icon><Plus /></el-icon>
            Add MCP Server
          </el-button>
        </div>
      </el-tab-pane>
    </el-tabs>

    <template #footer>
      <el-button @click="dialogVisible = false">Cancel</el-button>
      <el-button type="primary" @click="handleSave" :loading="saving">
        Save
      </el-button>
    </template>
  </el-dialog>

  <!-- Add MCP Server Dialog -->
  <el-dialog
    v-model="showAddMcpDialog"
    title="Add MCP Server"
    width="500px"
  >
    <el-form :model="newMcpServer" label-width="100px">
      <el-form-item label="Name">
        <el-input v-model="newMcpServer.name" placeholder="Server name" />
      </el-form-item>

      <el-form-item label="Transport">
        <el-radio-group v-model="newMcpServer.transportType">
          <el-radio value="stdio">Stdio</el-radio>
          <el-radio value="http">HTTP</el-radio>
        </el-radio-group>
      </el-form-item>

      <template v-if="newMcpServer.transportType === 'stdio'">
        <el-form-item label="Command">
          <el-input v-model="newMcpServer.command" placeholder="python" />
        </el-form-item>
        <el-form-item label="Arguments">
          <el-input
            v-model="newMcpServer.args"
            placeholder="-m mcp_server"
          />
        </el-form-item>
      </template>

      <template v-else>
        <el-form-item label="URL">
          <el-input v-model="newMcpServer.url" placeholder="http://localhost:3000" />
        </el-form-item>
      </template>
    </el-form>

    <template #footer>
      <el-button @click="showAddMcpDialog = false">Cancel</el-button>
      <el-button type="primary" @click="addMcpServer">Add</el-button>
    </template>
  </el-dialog>
</template>

<script setup lang="ts">
import { ref, watch, computed } from 'vue'
import { useConfigStore } from '@/stores/config'
import { ElMessage } from 'element-plus'
import { Folder, Plus, Delete } from '@element-plus/icons-vue'
import { invoke } from '@tauri-apps/api/core'

interface Props {
  modelValue?: boolean
}

interface Emits {
  (e: 'update:modelValue', value: boolean): void
}

const props = defineProps<Props>()
const emit = defineEmits<Emits>()

const configStore = useConfigStore()
const activeTab = ref('api')
const validating = ref(false)
const saving = ref(false)
const showAddMcpDialog = ref(false)

const localConfig = ref({
  api_key: '',
  api_base: 'https://open.bigmodel.cn/api/paas/v4',
  model: 'glm-4.7',
  system_prompt: '',
  work_directory: '',
  theme: 'dark',
  mcp_servers: []
})

const newMcpServer = ref({
  name: '',
  transportType: 'stdio',
  command: '',
  args: '',
  url: ''
})

const dialogVisible = computed({
  get: () => props.modelValue ?? false,
  set: (val) => emit('update:modelValue', val)
})

watch(() => props.modelValue, (val) => {
  if (val) {
    // Load current config
    localConfig.value = {
      api_key: '',
      api_base: 'https://open.bigmodel.cn/api/paas/v4',
      model: 'glm-4.7',
      system_prompt: '',
      work_directory: '',
      theme: 'dark',
      mcp_servers: [],
      ...configStore.config
    }
  }
})

async function handleValidate() {
  if (!localConfig.value.api_key) {
    ElMessage.warning('Please enter an API key first')
    return
  }

  validating.value = true
  try {
    const valid = await configStore.validateApiKey(
      localConfig.value.api_key,
      localConfig.value.api_base
    )
    if (valid) {
      ElMessage.success('API key is valid')
    } else {
      ElMessage.error('Invalid API key')
    }
  } catch (error) {
    ElMessage.error('Failed to validate API key')
  } finally {
    validating.value = false
  }
}

async function handleSelectFolder() {
  try {
    const path = await invoke<string | null>('select_folder')
    if (path) {
      localConfig.value.work_directory = path
    }
  } catch (error) {
    ElMessage.error('Failed to select folder')
  }
}

async function handleSave() {
  saving.value = true
  try {
    await configStore.saveConfig(localConfig.value as any)
    ElMessage.success('Settings saved successfully')
    dialogVisible.value = false
  } catch (error) {
    ElMessage.error('Failed to save settings')
  } finally {
    saving.value = false
  }
}

function addMcpServer() {
  if (!newMcpServer.value.name) {
    ElMessage.warning('Please enter a server name')
    return
  }

  const server: any = {
    name: newMcpServer.value.name,
    enabled: true,
    transport: {}
  }

  if (newMcpServer.value.transportType === 'stdio') {
    server.transport = {
      type: 'stdio',
      command: newMcpServer.value.command,
      args: newMcpServer.value.args.split(' ').filter(Boolean)
    }
  } else {
    server.transport = {
      type: 'http',
      url: newMcpServer.value.url
    }
  }

  localConfig.value.mcp_servers.push(server)
  showAddMcpDialog.value = false

  // Reset form
  newMcpServer.value = {
    name: '',
    transportType: 'stdio',
    command: '',
    args: '',
    url: ''
  }
}

function removeMcpServer(index: number) {
  localConfig.value.mcp_servers.splice(index, 1)
}
</script>

<style scoped>
.mcp-list {
  max-height: 400px;
  overflow-y: auto;
}

.mcp-item {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 12px;
  border: 1px solid var(--color-border);
  border-radius: 6px;
  margin-bottom: 8px;
}

.mcp-info {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.mcp-type {
  font-size: 12px;
  color: var(--color-text-secondary);
}

.mcp-actions {
  display: flex;
  align-items: center;
  gap: 12px;
}
</style>
