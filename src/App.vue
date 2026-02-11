<template>
  <div class="app-container">
    <!-- Three-column WeChat-style layout -->
    <div class="layout-wrapper">
      <!-- Sidebar: Conversation List -->
      <Sidebar class="sidebar" />

      <!-- Main: Chat Panel -->
      <ChatPanel class="chat-panel" />

      <!-- Right: Info Panel -->
      <InfoPanel class="info-panel" />
    </div>

    <!-- Settings Dialog -->
    <SettingsDialog />
  </div>
</template>

<script setup lang="ts">
import { onMounted } from 'vue'
import Sidebar from './components/Sidebar/index.vue'
import ChatPanel from './components/ChatPanel/index.vue'
import InfoPanel from './components/InfoPanel/index.vue'
import SettingsDialog from './components/Settings/index.vue'
import { useConfigStore } from './stores/config'

const configStore = useConfigStore()

onMounted(async () => {
  await configStore.loadConfig()
})
</script>

<style scoped>
.app-container {
  width: 100%;
  height: 100%;
  display: flex;
  flex-direction: column;
}

.layout-wrapper {
  display: flex;
  width: 100%;
  height: 100%;
  overflow: hidden;
}

.sidebar {
  width: 260px;
  min-width: 260px;
  background-color: var(--color-surface);
  border-right: 1px solid var(--color-border);
}

.chat-panel {
  flex: 1;
  min-width: 0;
  background-color: var(--color-bg);
}

.info-panel {
  width: 280px;
  min-width: 280px;
  background-color: var(--color-surface);
  border-left: 1px solid var(--color-border);
}
</style>
