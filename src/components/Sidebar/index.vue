<template>
  <div class="sidebar-container">
    <!-- Header -->
    <div class="sidebar-header">
      <div class="user-avatar">
        <el-icon :size="24"><User /></el-icon>
      </div>
      <div class="header-actions">
        <el-button text circle size="small" @click="handleNewChat">
          <el-icon><Plus /></el-icon>
        </el-button>
        <el-button text circle size="small" @click="showSettings = true">
          <el-icon><Setting /></el-icon>
        </el-button>
      </div>
    </div>

    <!-- Search -->
    <div class="sidebar-search">
      <el-input
        v-model="searchQuery"
        placeholder="Search conversations..."
        size="small"
        clearable
      >
        <template #prefix>
          <el-icon><Search /></el-icon>
        </template>
      </el-input>
    </div>

    <!-- Conversation List -->
    <div class="conversation-list">
      <div
        v-for="conv in filteredConversations"
        :key="conv.id"
        class="conversation-item"
        :class="{ active: conv.id === chatStore.currentConversationId }"
        @click="handleSelectConversation(conv.id)"
      >
        <div class="conversation-avatar">
          <el-icon :size="20"><ChatDotRound /></el-icon>
        </div>
        <div class="conversation-info">
          <div class="conversation-title">{{ conv.title }}</div>
          <div class="conversation-preview">{{ conv.model }}</div>
        </div>
        <el-dropdown trigger="click" @command="(cmd) => handleCommand(cmd, conv.id)">
          <el-icon class="more-icon"><MoreFilled /></el-icon>
          <template #dropdown>
            <el-dropdown-menu>
              <el-dropdown-item command="delete">
                <el-icon><Delete /></el-icon>
                Delete
              </el-dropdown-item>
            </el-dropdown-menu>
          </template>
        </el-dropdown>
      </div>

      <el-empty
        v-if="filteredConversations.length === 0"
        description="No conversations yet"
        :image-size="60"
      />
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, computed, onMounted } from 'vue'
import { useChatStore } from '@/stores/chat'
import { ElMessageBox } from 'element-plus'
import {
  User, Plus, Setting, Search, ChatDotRound, MoreFilled, Delete
} from '@element-plus/icons-vue'

const chatStore = useChatStore()
const searchQuery = ref('')
const showSettings = ref(false)

const filteredConversations = computed(() => {
  if (!searchQuery.value) return chatStore.conversations
  return chatStore.conversations.filter(c =>
    c.title.toLowerCase().includes(searchQuery.value.toLowerCase())
  )
})

async function handleNewChat() {
  const title = `New Chat ${chatStore.conversations.length + 1}`
  const model = 'gpt-4o-mini'
  const conv = await chatStore.createConversation(title, model)
  chatStore.setCurrentConversation(conv.id)
}

async function handleSelectConversation(id: string) {
  chatStore.setCurrentConversation(id)
  await chatStore.loadMessages(id)
}

async function handleCommand(command: string, id: string) {
  if (command === 'delete') {
    await ElMessageBox.confirm(
      'Are you sure you want to delete this conversation?',
      'Confirm Delete',
      {
        confirmButtonText: 'Delete',
        cancelButtonText: 'Cancel',
        type: 'warning',
      }
    )
    await chatStore.deleteConversation(id)
  }
}

onMounted(() => {
  chatStore.loadConversations()
})
</script>

<style scoped>
.sidebar-container {
  display: flex;
  flex-direction: column;
  height: 100%;
  background-color: var(--color-surface);
}

.sidebar-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 16px;
  border-bottom: 1px solid var(--color-border);
}

.user-avatar {
  width: 40px;
  height: 40px;
  display: flex;
  align-items: center;
  justify-content: center;
  background-color: var(--color-primary);
  border-radius: 4px;
  color: white;
}

.header-actions {
  display: flex;
  gap: 4px;
}

.sidebar-search {
  padding: 12px;
  border-bottom: 1px solid var(--color-border);
}

.conversation-list {
  flex: 1;
  overflow-y: auto;
  padding: 8px;
}

.conversation-item {
  display: flex;
  align-items: center;
  gap: 12px;
  padding: 10px 12px;
  border-radius: 6px;
  cursor: pointer;
  transition: background-color 0.2s;
}

.conversation-item:hover {
  background-color: var(--color-surface-hover);
}

.conversation-item.active {
  background-color: var(--color-surface-hover);
}

.conversation-avatar {
  width: 36px;
  height: 36px;
  display: flex;
  align-items: center;
  justify-content: center;
  background-color: var(--color-border);
  border-radius: 4px;
  flex-shrink: 0;
  color: var(--color-text-secondary);
}

.conversation-item.active .conversation-avatar {
  background-color: var(--color-primary);
  color: white;
}

.conversation-info {
  flex: 1;
  min-width: 0;
}

.conversation-title {
  font-size: 14px;
  font-weight: 500;
  color: var(--color-text);
  text-overflow: ellipsis;
  overflow: hidden;
  white-space: nowrap;
}

.conversation-preview {
  font-size: 12px;
  color: var(--color-text-secondary);
  text-overflow: ellipsis;
  overflow: hidden;
  white-space: nowrap;
}

.more-icon {
  color: var(--color-text-secondary);
  cursor: pointer;
  opacity: 0;
  transition: opacity 0.2s;
}

.conversation-item:hover .more-icon {
  opacity: 1;
}
</style>
