<template>
  <div class="skills-manager">
    <div class="skills-header">
      <h3>Skills</h3>
      <el-button size="small" @click="showMarket = true">
        <el-icon><ShoppingBag /></el-icon>
        Browse Market
      </el-button>
    </div>

    <div class="skills-list">
      <div
        v-for="skill in skills"
        :key="skill.id"
        class="skill-card"
      >
        <div class="skill-info">
          <h4>{{ skill.name }}</h4>
          <p class="skill-description">{{ skill.description }}</p>
          <div class="skill-meta">
            <span class="skill-version">v{{ skill.version }}</span>
            <span class="skill-author">by {{ skill.author }}</span>
          </div>
        </div>
        <div class="skill-actions">
          <el-switch v-model="skill.enabled" size="small" />
          <el-button type="danger" size="small" text @click="handleUninstall(skill.id)">
            <el-icon><Delete /></el-icon>
          </el-button>
        </div>
      </div>

      <el-empty
        v-if="skills.length === 0"
        description="No skills installed"
        :image-size="60"
      />
    </div>

    <!-- Skills Market Dialog -->
    <el-dialog
      v-model="showMarket"
      title="Skills Market"
      width="700px"
    >
      <div class="market-search">
        <el-input
          v-model="searchQuery"
          placeholder="Search skills..."
          prefix-icon="Search"
        />
      </div>

      <div class="market-list">
        <div
          v-for="skill in availableSkills"
          :key="skill.id"
          class="market-item"
        >
          <div class="market-info">
            <h4>{{ skill.name }}</h4>
            <p>{{ skill.description }}</p>
            <div class="market-meta">
              <el-tag size="small">{{ skill.category }}</el-tag>
              <span class="market-author">{{ skill.author }}</span>
            </div>
          </div>
          <el-button
            type="primary"
            size="small"
            @click="handleInstall(skill)"
            :loading="installing === skill.id"
          >
            Install
          </el-button>
        </div>
      </div>
    </el-dialog>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { ShoppingBag, Delete } from '@element-plus/icons-vue'
import { ElMessage } from 'element-plus'

const skills = ref<any[]>([])
const showMarket = ref(false)
const searchQuery = ref('')
const installing = ref<string | null>(null)

// Mock data for available skills
const availableSkills = ref([
  {
    id: 'code-analysis',
    name: 'Code Analysis',
    description: 'Analyze code quality and find potential issues',
    author: 'PETool',
    category: 'Development',
    repo: 'https://github.com/petool/skill-code-analysis'
  },
  {
    id: 'git-ops',
    name: 'Git Operations',
    description: 'Execute Git commands and view history',
    author: 'PETool',
    category: 'Development',
    repo: 'https://github.com/petool/skill-git-ops'
  },
  {
    id: 'file-search',
    name: 'File Search',
    description: 'Search file contents in your project',
    author: 'PETool',
    category: 'Utilities',
    repo: 'https://github.com/petool/skill-file-search'
  }
])

async function handleInstall(skill: any) {
  installing.value = skill.id
  try {
    // TODO: Implement actual skill installation
    await new Promise(resolve => setTimeout(resolve, 1000))
    ElMessage.success(`Skill "${skill.name}" installed successfully`)
    showMarket.value = false
  } catch (error) {
    ElMessage.error('Failed to install skill')
  } finally {
    installing.value = null
  }
}

async function handleUninstall(id: string) {
  // TODO: Implement actual skill uninstallation
  skills.value = skills.value.filter(s => s.id !== id)
  ElMessage.success('Skill uninstalled')
}
</script>

<style scoped>
.skills-manager {
  height: 100%;
  display: flex;
  flex-direction: column;
}

.skills-header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 16px;
}

.skills-header h3 {
  margin: 0;
  font-size: 16px;
  font-weight: 500;
}

.skills-list {
  flex: 1;
  overflow-y: auto;
}

.skill-card {
  display: flex;
  justify-content: space-between;
  align-items: flex-start;
  padding: 12px;
  border: 1px solid var(--color-border);
  border-radius: 6px;
  margin-bottom: 8px;
}

.skill-info h4 {
  margin: 0 0 4px 0;
  font-size: 14px;
  font-weight: 500;
}

.skill-description {
  margin: 0 0 8px 0;
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
  gap: 8px;
}

.market-search {
  margin-bottom: 16px;
}

.market-list {
  max-height: 400px;
  overflow-y: auto;
}

.market-item {
  display: flex;
  justify-content: space-between;
  align-items: center;
  padding: 12px;
  border: 1px solid var(--color-border);
  border-radius: 6px;
  margin-bottom: 8px;
}

.market-info h4 {
  margin: 0 0 4px 0;
  font-size: 14px;
}

.market-info p {
  margin: 0 0 8px 0;
  font-size: 12px;
  color: var(--color-text-secondary);
}

.market-meta {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 11px;
  color: var(--color-text-secondary);
}
</style>
