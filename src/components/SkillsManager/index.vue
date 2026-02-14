<template>
  <div class="skills-manager">
    <div class="skills-header">
      <h3>Skills</h3>
      <el-button size="small" @click="loadSkills" :loading="loading">
        Refresh
      </el-button>
    </div>

    <div class="install-row">
      <el-input
        v-model="repoUrl"
        placeholder="https://github.com/owner/repo.git"
        clearable
      />
      <el-button type="primary" :loading="installingByUrl" @click="installByUrl">
        Install
      </el-button>
    </div>

    <div class="discover-row">
      <el-input
        v-model="discoverQuery"
        placeholder="Search skills (e.g. word docx export)"
        clearable
      />
      <el-button :loading="discovering" @click="discoverSkills">
        Discover
      </el-button>
    </div>

    <div v-if="discoveredSkills.length > 0" class="discover-list">
      <div
        v-for="item in discoveredSkills"
        :key="item.id"
        class="discover-item"
      >
        <div class="market-info">
          <h4>{{ item.name }}</h4>
          <p>{{ item.description || item.repo_full_name }}</p>
          <p class="discover-meta">
            {{ item.repo_full_name }} · ★{{ item.stars }}
            <span v-if="item.skill_path"> · path: {{ item.skill_path }}</span>
          </p>
        </div>
        <el-button
          size="small"
          :disabled="item.installed"
          :loading="installing === `${item.repo_url}#${item.skill_path || '.'}`"
          @click="installFromDiscovery(item)"
        >
          {{ item.installed ? 'Installed' : 'Install' }}
        </el-button>
      </div>
    </div>

    <div class="market-list">
      <div
        v-for="item in suggestedSkills"
        :key="item.id"
        class="market-item"
      >
        <div class="market-info">
          <h4>{{ item.name }}</h4>
          <p>{{ item.description }}</p>
        </div>
        <el-button size="small" :loading="installing === item.repo" @click="installFromRepo(item.repo)">
          Install
        </el-button>
      </div>
    </div>

    <div class="skills-list">
      <div
        v-for="skill in skills"
        :key="skill.id"
        class="skill-card"
      >
        <div class="skill-info">
          <h4>{{ skill.name }}</h4>
          <p class="skill-description">{{ skill.description || 'No description' }}</p>
          <div class="skill-meta">
            <span class="skill-version">v{{ skill.version }}</span>
            <span class="skill-author">by {{ skill.author || 'unknown' }}</span>
          </div>
        </div>
        <div class="skill-actions">
          <el-switch
            :model-value="skill.enabled"
            size="small"
            @change="handleToggleChange(skill, $event)"
          />
          <el-button size="small" text @click="handleUpdate(skill.id)" :loading="updating === skill.id">
            Update
          </el-button>
          <el-button type="danger" size="small" text @click="handleUninstall(skill.id)" :loading="removing === skill.id">
            Remove
          </el-button>
        </div>
      </div>

      <el-empty
        v-if="!loading && skills.length === 0"
        description="No skills installed"
        :image-size="60"
      />
    </div>
  </div>
</template>

<script setup lang="ts">
import { onMounted, ref } from 'vue'
import { ElMessage } from 'element-plus'
import { invoke } from '@tauri-apps/api/core'

interface Skill {
  id: string
  name: string
  version: string
  description: string
  author: string
  enabled: boolean
  installed_at: string
  script_type: 'rust' | 'javascript' | 'markdown'
}

interface SkillDiscoveryItem {
  id: string
  name: string
  description: string
  repo_url: string
  repo_full_name: string
  repo_html_url: string
  source: string
  skill_path?: string | null
  stars: number
  updated_at?: string | null
  installed: boolean
}

const loading = ref(false)
const installingByUrl = ref(false)
const installing = ref<string | null>(null)
const discovering = ref(false)
const removing = ref<string | null>(null)
const updating = ref<string | null>(null)
const repoUrl = ref('')
const discoverQuery = ref('')
const skills = ref<Skill[]>([])
const discoveredSkills = ref<SkillDiscoveryItem[]>([])

const suggestedSkills = [
  {
    id: 'skill-installer',
    name: 'Skill Installer',
    description: 'Install and manage Codex skills',
    repo: 'https://github.com/openclaw/openclaw.git'
  },
  {
    id: 'file-analyzer',
    name: 'File Analyzer',
    description: 'Parse and analyze code files quickly',
    repo: 'https://github.com/sst/opencode.git'
  }
]

async function loadSkills() {
  loading.value = true
  try {
    skills.value = await invoke<Skill[]>('list_skills')
  } catch (error) {
    ElMessage.error('Failed to load skills')
  } finally {
    loading.value = false
  }
}

async function installFromRepo(repo: string) {
  installing.value = repo
  try {
    await invoke('install_skill', { repoUrl: repo })
    ElMessage.success('Skill installed')
    await loadSkills()
  } catch (error: unknown) {
    const message = error instanceof Error ? error.message : 'Failed to install skill'
    ElMessage.error(message)
  } finally {
    installing.value = null
  }
}

async function discoverSkills() {
  discovering.value = true
  try {
    discoveredSkills.value = await invoke<SkillDiscoveryItem[]>('discover_skills', {
      query: discoverQuery.value.trim() || null,
      limit: 8
    })
  } catch (error: unknown) {
    const message = error instanceof Error ? error.message : 'Failed to discover skills'
    ElMessage.error(message)
  } finally {
    discovering.value = false
  }
}

async function installFromDiscovery(item: SkillDiscoveryItem) {
  installing.value = `${item.repo_url}#${item.skill_path || '.'}`
  try {
    await invoke('install_skill', {
      repoUrl: item.repo_url,
      skillPath: item.skill_path ?? null
    })
    ElMessage.success('Skill installed')
    await Promise.all([loadSkills(), discoverSkills()])
  } catch (error: unknown) {
    const message = error instanceof Error ? error.message : 'Failed to install skill'
    ElMessage.error(message)
  } finally {
    installing.value = null
  }
}

async function installByUrl() {
  const url = repoUrl.value.trim()
  if (!url) {
    ElMessage.warning('Please enter a repository URL')
    return
  }

  installingByUrl.value = true
  try {
    await invoke('install_skill', { repoUrl: url })
    ElMessage.success('Skill installed')
    repoUrl.value = ''
    await loadSkills()
  } catch (error: unknown) {
    const message = error instanceof Error ? error.message : 'Failed to install skill'
    ElMessage.error(message)
  } finally {
    installingByUrl.value = false
  }
}

async function handleToggle(skill: Skill, enabled: boolean) {
  const previous = skill.enabled
  skill.enabled = enabled
  try {
    await invoke('toggle_skill', { skillId: skill.id, enabled })
    ElMessage.success(enabled ? 'Skill enabled' : 'Skill disabled')
  } catch (error) {
    skill.enabled = previous
    ElMessage.error('Failed to toggle skill')
  }
}

function handleToggleChange(skill: Skill, value: string | number | boolean) {
  void handleToggle(skill, Boolean(value))
}

async function handleUninstall(skillId: string) {
  removing.value = skillId
  try {
    await invoke('uninstall_skill', { skillId })
    ElMessage.success('Skill removed')
    await loadSkills()
  } catch (error: unknown) {
    const message = error instanceof Error ? error.message : 'Failed to remove skill'
    ElMessage.error(message)
  } finally {
    removing.value = null
  }
}

async function handleUpdate(skillId: string) {
  updating.value = skillId
  try {
    await invoke('update_skill', { skillId })
    ElMessage.success('Skill updated')
    await loadSkills()
  } catch (error: unknown) {
    const message = error instanceof Error ? error.message : 'Failed to update skill'
    ElMessage.error(message)
  } finally {
    updating.value = null
  }
}

onMounted(() => {
  void loadSkills()
})
</script>

<style scoped>
.skills-manager {
  height: 100%;
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.skills-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
}

.skills-header h3 {
  margin: 0;
  font-size: 15px;
  font-weight: 600;
}

.install-row {
  display: grid;
  grid-template-columns: 1fr auto;
  gap: 8px;
}

.discover-row {
  display: grid;
  grid-template-columns: 1fr auto;
  gap: 8px;
}

.discover-list {
  display: grid;
  gap: 8px;
}

.discover-item {
  border: 1px solid var(--color-border);
  border-radius: 6px;
  padding: 10px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
}

.discover-meta {
  font-size: 11px;
  color: var(--color-text-secondary);
}

.market-list {
  display: grid;
  gap: 8px;
}

.market-item {
  border: 1px solid var(--color-border);
  border-radius: 6px;
  padding: 10px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
}

.market-info h4 {
  margin: 0 0 4px 0;
  font-size: 13px;
}

.market-info p {
  margin: 0;
  font-size: 12px;
  color: var(--color-text-secondary);
}

.skills-list {
  flex: 1;
  overflow-y: auto;
}

.skill-card {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  border: 1px solid var(--color-border);
  border-radius: 6px;
  padding: 10px;
  margin-bottom: 8px;
  gap: 10px;
}

.skill-info h4 {
  margin: 0 0 4px 0;
  font-size: 14px;
}

.skill-description {
  margin: 0 0 6px 0;
  font-size: 12px;
  color: var(--color-text-secondary);
}

.skill-meta {
  display: flex;
  gap: 8px;
  font-size: 11px;
  color: var(--color-text-secondary);
}

.skill-actions {
  display: flex;
  flex-direction: column;
  align-items: flex-end;
  gap: 4px;
}
</style>
