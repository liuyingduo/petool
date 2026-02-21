<template>
  <aside class="sidebar">
    <button class="new-btn" @click="$emit('create')">
      <span class="material-symbols-outlined">add_circle</span>
      开启新冒险
    </button>

    <div class="sidebar-title">进行中</div>

    <div class="conversation-list no-scrollbar" v-memo="memoDeps">
      <div
        v-for="conv in conversations"
        :key="conv.id"
        class="conv-item-row"
        :class="{ active: conv.id === activeConversationId }"
      >
        <button
          class="conv-item"
          :class="{ active: conv.id === activeConversationId }"
          @click="$emit('select', conv.id)"
        >
          <span class="dot"></span>
          <span class="conv-title">{{ conv.title }}</span>
        </button>
        <div class="conv-menu-anchor">
          <el-dropdown
            trigger="click"
            placement="bottom-end"
            popper-class="conv-actions-menu"
            @command="$emit('command', conv.id, $event)"
          >
            <button
              class="conv-menu-trigger"
              type="button"
              title="会话操作"
              aria-label="会话操作"
              :disabled="streamingStatusMap[conv.id]"
              @click.stop
            >
              <span class="conv-menu-dot" aria-hidden="true"></span>
              <span class="conv-menu-dot" aria-hidden="true"></span>
              <span class="conv-menu-dot" aria-hidden="true"></span>
            </button>
            <template #dropdown>
              <el-dropdown-menu>
                <el-dropdown-item command="pin">
                  {{ pinnedIds.has(conv.id) ? '取消置顶' : '置顶' }}
                </el-dropdown-item>
                <el-dropdown-item command="rename">重命名</el-dropdown-item>
                <el-dropdown-item command="delete" class="danger" divided>删除</el-dropdown-item>
              </el-dropdown-menu>
            </template>
          </el-dropdown>
        </div>
      </div>

      <div v-if="!hasConversations" class="empty-tip">还没有任务，先创建一个吧。</div>
    </div>

    <div class="sidebar-footer">
      <button
        class="sidebar-user-card account-entry-btn"
        type="button"
        title="账户中心"
        @click="$emit('account')"
      >
        <div class="sidebar-user-meta">
          <div class="sidebar-avatar">
            <img :src="userAvatar" alt="User Avatar" />
          </div>
          <div class="sidebar-user-text">
            <span class="name">{{ userName }}</span>
            <span class="plan">{{ userPlan }}</span>
          </div>
        </div>
      </button>
      <button class="sidebar-settings-btn" @click="$emit('settings')" title="系统设置">
        <span class="material-icons-round">settings</span>
      </button>
    </div>
  </aside>
</template>

<script setup lang="ts">
import { computed } from 'vue'

export interface ConversationItem {
  id: string
  title: string
}

const props = defineProps<{
  conversations: ConversationItem[]
  activeConversationId: string | null
  pinnedIds: Set<string>
  streamingStatusMap: Record<string, boolean>
  userAvatar: string
  userName: string
  userPlan: string
  memoDeps: unknown[]
}>()

const emit = defineEmits<{
  (e: 'create'): void
  (e: 'select', id: string): void
  (e: 'command', id: string, command: string | number | object): void
  (e: 'account'): void
  (e: 'settings'): void
}>()

const hasConversations = computed(() => {
  return props.conversations && props.conversations.length > 0
})
</script>

<style scoped>
.sidebar {
  width: 240px;
  border-right: 1px solid rgba(214, 211, 209, 0.7);
  background: #f3eee6;
  display: flex;
  flex-direction: column;
  padding: 62px 16px 20px;
  gap: 16px;
  border-radius: 0;
  overflow: hidden;
}

.new-btn {
  height: 48px;
  border: none;
  border-radius: 16px;
  background: #4a7c59;
  color: #FFFFFF;
  font-weight: 700;
  font-size: 14px;
  display: flex;
  align-items: center;
  justify-content: center;
  gap: 8px;
  cursor: pointer;
  transition: transform 0.15s ease, background-color 0.2s ease, box-shadow 0.2s ease;
  box-shadow: 0 8px 20px -8px rgba(74, 124, 89, 0.5);
}

.new-btn span {
  font-size: 14px;
  letter-spacing: 0.03em;
}

.new-btn .material-symbols-outlined {
  font-size: 27px;
}

.new-btn:hover {
  background: #6b9c7a;
  transform: translateY(-1px);
}

.sidebar-title {
  font-size: 12px;
  color: #a8a29e;
  text-transform: uppercase;
  letter-spacing: 0.05em;
  margin-top: 8px;
  font-weight: 700;
}

.conversation-list {
  flex: 1;
  overflow-y: auto;
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.conv-item-row {
  position: relative;
}

.conv-item {
  width: 100%;
  border: none;
  background: transparent;
  border-radius: 12px;
  padding: 8px 36px 8px 10px;
  display: flex;
  align-items: center;
  gap: 10px;
  color: #6B7280;
  cursor: pointer;
  text-align: left;
  transition: background-color 0.2s ease;
}

.conv-item.active {
  background: #FFFFFF;
  color: #2c3e33;
  box-shadow: 0 4px 12px rgba(0, 0, 0, 0.03);
}

.dot {
  width: 7px;
  height: 7px;
  border-radius: 999px;
  background: transparent;
  flex-shrink: 0;
}

.conv-item.active .dot {
  background: #4a7c59;
}

.conv-title {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  font-size: 13px;
  font-weight: 600;
}

.conv-item .material-icons-round {
  font-size: 18px;
  color: #9a948e;
}

.conv-item.active .material-icons-round {
  color: #4a7c59;
}

.conv-menu-anchor {
  position: absolute;
  top: 50%;
  right: 10px;
  transform: translateY(-50%);
  z-index: 2;
}

.conv-menu-trigger {
  width: 28px;
  height: 28px;
  border: none;
  border-radius: 999px;
  background: transparent;
  color: #b7b0a8;
  display: inline-flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 2px;
  opacity: 0;
  pointer-events: none;
  transition: opacity 0.2s ease, background-color 0.2s ease, color 0.2s ease;
  padding: 0;
}

.conv-menu-dot {
  width: 3px;
  height: 3px;
  border-radius: 999px;
  background: currentColor;
}

.conv-item-row:hover .conv-menu-trigger,
.conv-item-row:focus-within .conv-menu-trigger,
.conv-item-row.active .conv-menu-trigger {
  opacity: 1;
}

.conv-menu-trigger:hover:not(:disabled) {
  background: #e7e2da;
  color: #5f574d;
  opacity: 1;
  pointer-events: auto;
}

.conv-menu-trigger:disabled {
  opacity: 0.35;
  cursor: not-allowed;
}

.conv-item-row:hover .conv-menu-trigger,
.conv-item-row:focus-within .conv-menu-trigger,
.conv-item-row.active .conv-menu-trigger {
  pointer-events: auto;
}

.empty-tip {
  color: #9ca3af;
  font-size: 12px;
  padding: 8px;
}

.sidebar-footer {
  border-top: 1px solid rgba(214, 211, 209, 0.7);
  padding-top: 14px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
}

.account-entry-btn {
  flex: 1;
  border: none;
  background: transparent;
  border-radius: 10px;
  padding: 0;
  text-align: left;
  cursor: pointer;
  transition: background-color 0.2s ease, opacity 0.2s ease;
}

.account-entry-btn:hover {
  background: rgba(255, 255, 255, 0.35);
}

.sidebar-user-card {
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 10px;
}

.sidebar-user-meta {
  display: flex;
  align-items: center;
  gap: 10px;
  min-width: 0;
}

.sidebar-avatar {
  width: 36px;
  height: 36px;
  border-radius: 999px;
  overflow: hidden;
  background: #d6d3d1;
  border: 2px solid #fff;
  box-shadow: 0 6px 15px -8px rgba(0, 0, 0, 0.3);
  flex-shrink: 0;
}

.sidebar-avatar img {
  width: 100%;
  height: 100%;
  object-fit: cover;
}

.sidebar-user-text {
  display: flex;
  flex-direction: column;
  min-width: 0;
}

.sidebar-user-text .name {
  font-size: 14px;
  font-weight: 800;
  color: #44403c;
  line-height: 1.1;
}

.sidebar-user-text .plan {
  margin-top: 2px;
  font-size: 10px;
  font-weight: 700;
  color: #a8a29e;
}

.sidebar-settings-btn {
  border: 0;
  background: transparent;
  color: #4a7c59;
  width: 32px;
  height: 32px;
  border-radius: 999px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
}

@media (max-width: 900px) {
  .sidebar {
    width: 100%;
    max-height: 230px;
    border-right: none;
    border-bottom: 1px solid rgba(214, 211, 209, 0.7);
    border-radius: 0;
    padding-top: 16px;
  }
}
</style>
