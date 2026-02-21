<template>
  <div class="task-monitor-shell" :class="{ collapsed }">
    <button class="task-monitor-toggle" type="button" @click="$emit('toggle')">
      <span class="material-icons-round">
        {{ collapsed ? 'keyboard_arrow_left' : 'keyboard_arrow_right' }}
      </span>
    </button>
    <aside class="task-monitor-card">
      <div class="task-monitor-title">Task Monitor</div>

      <section class="task-monitor-section">
        <button class="task-monitor-section-head" type="button" @click="toggleMonitorSection('todos')">
          <span>Todos</span>
          <span class="material-icons-round">{{ monitorSectionsOpen.todos ? 'expand_less' : 'expand_more' }}</span>
        </button>
        <div v-show="monitorSectionsOpen.todos" class="task-monitor-section-body">
          <div v-if="todos.length === 0" class="task-monitor-empty">等待工具执行...</div>
          <div v-for="todo in todos" :key="todo.id" class="task-monitor-row">
            <span class="status-indicator" :class="todo.status" aria-hidden="true">
              <span v-if="todo.status === 'running'" class="status-spinner"></span>
              <span v-else-if="todo.status === 'done'" class="material-icons-round">check_circle</span>
              <span v-else class="material-icons-round">cancel</span>
            </span>
            <span class="task-monitor-row-label">{{ todo.label }}</span>
          </div>
        </div>
      </section>

      <section class="task-monitor-section">
        <button class="task-monitor-section-head" type="button" @click="toggleMonitorSection('artifacts')">
          <span>Artifacts</span>
          <span class="material-icons-round">{{ monitorSectionsOpen.artifacts ? 'expand_less' : 'expand_more' }}</span>
        </button>
        <div v-show="monitorSectionsOpen.artifacts" class="task-monitor-section-body">
          <div v-if="artifacts.length === 0" class="task-monitor-empty">
            {{ currentDirectory ? truncateMiddle(currentDirectory, 38) : 'Default workspace' }}
          </div>
          <div v-for="artifact in artifacts" :key="artifact.id" class="task-monitor-row artifact">
            <span class="status-indicator" :class="artifact.status" aria-hidden="true">
              <span v-if="artifact.status === 'running'" class="status-spinner"></span>
              <span v-else-if="artifact.status === 'done'" class="material-icons-round">check_circle</span>
              <span v-else class="material-icons-round">cancel</span>
            </span>
            <div class="task-monitor-artifact-content">
              <div class="task-monitor-row-label">{{ artifact.name }}</div>
              <div class="task-monitor-row-sub">{{ artifact.action }} · {{ artifact.path }}</div>
            </div>
          </div>
        </div>
      </section>

      <section class="task-monitor-section">
        <button class="task-monitor-section-head" type="button" @click="toggleMonitorSection('skills')">
          <span>Skills & MCP</span>
          <span class="material-icons-round">{{ monitorSectionsOpen.skills ? 'expand_less' : 'expand_more' }}</span>
        </button>
        <div v-show="monitorSectionsOpen.skills" class="task-monitor-section-body">
          <div v-if="skills.length === 0" class="task-monitor-empty">暂无</div>
          <div v-for="skill in skills" :key="skill" class="task-monitor-row skill">
            <span class="material-icons-round">api</span>
            <span class="task-monitor-row-label">{{ skill }}</span>
          </div>
        </div>
      </section>
    </aside>
  </div>
</template>

<script setup lang="ts">
import { ref } from 'vue'
import { truncateMiddle } from '@/utils/toolDisplay'

export interface MonitorTodoItem {
  id: string
  label: string
  status: string // e.g. 'running' | 'done' | 'error'
}

export interface MonitorArtifactItem {
  id: string
  name: string
  path: string
  action: string
  status: string
}

defineProps<{
  collapsed: boolean
  todos: MonitorTodoItem[]
  artifacts: MonitorArtifactItem[]
  skills: string[]
  currentDirectory: string | null
}>()

defineEmits<{
  (e: 'toggle'): void
}>()

const monitorSectionsOpen = ref({
  todos: true,
  artifacts: true,
  skills: true
})

function toggleMonitorSection(section: 'todos' | 'artifacts' | 'skills') {
  monitorSectionsOpen.value[section] = !monitorSectionsOpen.value[section]
}
</script>

<style scoped>
.task-monitor-shell {
  position: absolute;
  top: var(--chat-content-top-safe);
  right: 16px;
  width: 286px;
  max-height: calc(100% - var(--chat-content-top-safe) - 8px);
  z-index: 20;
  transition: transform 0.24s ease;
}

.task-monitor-shell.collapsed {
  transform: translateX(calc(100% + 16px));
}

.task-monitor-toggle {
  position: absolute;
  top: 16px;
  left: -26px;
  width: 26px;
  height: 56px;
  border: 1px solid rgba(255, 255, 255, 0.8);
  border-right: none;
  border-radius: 14px 0 0 14px;
  background: rgba(255, 255, 255, 0.75);
  backdrop-filter: blur(20px);
  -webkit-backdrop-filter: blur(20px);
  color: #9ca3af;
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  box-shadow: -8px 12px 24px -12px rgba(44, 62, 51, 0.15);
  transition: box-shadow 0.2s ease, background-color 0.2s ease, color 0.2s ease;
}

.task-monitor-toggle:hover {
  background: rgba(255, 255, 255, 0.9);
  color: #57534e;
}

.task-monitor-toggle .material-icons-round {
  font-size: 20px;
}

.task-monitor-card {
  background: rgba(255, 255, 255, 0.75);
  backdrop-filter: blur(20px);
  -webkit-backdrop-filter: blur(20px);
  border: 1px solid rgba(255, 255, 255, 0.8);
  border-radius: 24px;
  box-shadow:
    0 20px 40px -12px rgba(44, 62, 51, 0.12),
    inset 0 1px 1px rgba(255, 255, 255, 0.9);
  overflow: hidden;
  max-height: 100%;
  overflow-y: auto;
  transition: opacity 0.2s ease, transform 0.3s cubic-bezier(0.16, 1, 0.3, 1);
  display: flex;
  flex-direction: column;
}

.task-monitor-shell.collapsed .task-monitor-card {
  opacity: 0;
  pointer-events: none;
}

.task-monitor-shell.collapsed .task-monitor-toggle {
  left: -26px;
  box-shadow: -4px 0 10px rgba(0, 0, 0, 0.05);
}

.task-monitor-title {
  font-size: 16px;
  font-weight: 800;
  color: #2c3e33;
  padding: 16px 18px 12px;
  letter-spacing: 0.02em;
}

.task-monitor-section {
  border-top: 1px solid rgba(234, 232, 228, 0.6);
}

.task-monitor-section-head {
  width: 100%;
  border: none;
  background: transparent;
  padding: 12px 18px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  cursor: pointer;
  color: #57534e;
  font-size: 13px;
  font-weight: 700;
  transition: background-color 0.2s ease;
}

.task-monitor-section-head:hover {
  background: rgba(255, 255, 255, 0.4);
}

.task-monitor-section-head .material-icons-round {
  font-size: 18px;
  color: #a8a29e;
  transition: transform 0.2s ease;
}

.task-monitor-section-body {
  padding: 0 18px 16px;
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.task-monitor-row {
  display: flex;
  align-items: flex-start;
  gap: 10px;
  min-height: 18px;
}

.task-monitor-row-label {
  font-size: 13px;
  color: #44403c;
  line-height: 1.5;
  word-break: break-word;
  font-weight: 600;
}

.task-monitor-row-sub {
  font-size: 11px;
  color: #9ca3af;
  line-height: 1.4;
  margin-top: 2px;
}

.task-monitor-artifact-content {
  min-width: 0;
  display: flex;
  flex-direction: column;
}

.task-monitor-empty {
  font-size: 12px;
  color: #9ca3af;
  line-height: 1.4;
}

.status-indicator {
  display: flex;
  align-items: center;
  justify-content: center;
  flex-shrink: 0;
  margin-top: 1px;
}

.status-indicator .material-icons-round {
  font-size: 16px;
  line-height: 1;
}

.status-indicator.running {
  color: #3b82f6;
}

.status-indicator.done {
  color: #4a7c59;
}

.status-indicator.error {
  color: #dc2626;
}

.status-spinner {
  width: 12px;
  height: 12px;
  border-radius: 999px;
  border: 2px solid rgba(59, 130, 246, 0.2);
  border-top-color: #3b82f6;
  animation: toolSpin 0.9s linear infinite;
}

.task-monitor-row .status-indicator.done .material-icons-round {
  animation: statusPop 0.25s ease-out;
}

@keyframes toolSpin {
  100% {
    transform: rotate(360deg);
  }
}

@keyframes statusPop {
  0% { transform: scale(0.6); opacity: 0; }
  60% { transform: scale(1.1); }
  100% { transform: scale(1); opacity: 1; }
}

.task-monitor-row.skill .material-icons-round {
  font-size: 16px;
  color: #6b7280;
  margin-top: 1px;
}

/* media queries */
@media (max-width: 1280px) {
  .task-monitor-shell {
    width: 260px;
    right: 10px;
    top: 84px;
    max-height: calc(100% - 104px);
  }
}

@media (max-width: 900px) {
  .task-monitor-shell {
    display: none;
  }
}
</style>
