<template>
  <div class="chat-input-wrapper">
    <div v-if="uploads.length > 0" class="upload-strip">
      <div class="upload-strip-title">已添加文件（发送后会一并交给模型）</div>
      <div class="upload-list">
        <div v-for="item in uploads" :key="item.id" class="upload-chip">
          <span class="material-icons-round">{{ uploadIcon(item.extension) }}</span>
          <span class="upload-chip-name">{{ item.name }}</span>
          <span class="upload-chip-meta">{{ formatBytes(item.size) }}</span>
          <button class="upload-chip-remove" type="button" @click.stop="$emit('removeUpload', item.id)">
            <span class="material-icons-round">close</span>
          </button>
        </div>
      </div>
    </div>

    <div class="input-bar" :class="{ disabled }">
      <div class="model-selector">
        <button
          class="model-trigger"
          type="button"
          :disabled="disabled || isStreaming"
          aria-label="选择模型"
        >
          <span class="model-dot"></span>
          <span class="model-text">{{ activeModelLabel }}</span>
          <span class="material-icons-round">expand_more</span>
        </button>
        <div class="model-dropdown">
          <div class="model-dropdown-title">选择模型</div>
          <button
            v-for="model in modelOptions"
            :key="model"
            class="model-option"
            type="button"
            :class="{ active: model === activeModelId }"
            @click="$emit('selectModel', model)"
          >
            <span>{{ formatModelLabel(model) }}</span>
            <span v-if="model === activeModelId" class="material-icons-round">check</span>
          </button>
        </div>
      </div>
      <button class="attach-btn" @click="$emit('selectUploadFiles')" :disabled="disabled || isStreaming">
        <span class="material-icons-round">attach_file</span>
      </button>

      <input
        ref="composerInputRef"
        type="text"
        placeholder="想让我做什么？"
        :disabled="disabled || isStreaming"
        spellcheck="false"
        autocomplete="off"
        :value="modelValue"
        @input="$emit('update:modelValue', ($event.target as HTMLInputElement).value)"
        @keydown.enter.prevent="onEnter"
      />
      <button
        class="send-btn"
        :disabled="disabled || (isStreaming ? isPausing : false)"
        @click="isStreaming ? $emit('pauseStream') : $emit('sendMessage')"
      >
        <span v-if="isStreaming" class="send-stop-square" aria-hidden="true"></span>
        <span v-else class="material-icons-round">arrow_upward</span>
      </button>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, watch } from 'vue'

export interface UploadAttachment {
  id: string
  path: string
  name: string
  extension: string
  size: number
}

const props = defineProps<{
  modelValue: string
  disabled: boolean
  isStreaming: boolean
  isPausing: boolean
  uploads: UploadAttachment[]
  activeModelId: string
  activeModelLabel: string
  modelOptions: string[]
  formatModelLabel: (id: string) => string
}>()

const emit = defineEmits<{
  (e: 'update:modelValue', val: string): void
  (e: 'sendMessage'): void
  (e: 'pauseStream'): void
  (e: 'selectUploadFiles'): void
  (e: 'removeUpload', id: string): void
  (e: 'selectModel', modelId: string): void
}>()

const composerInputRef = ref<HTMLInputElement | null>(null)

function onEnter() {
  if (props.disabled || props.isStreaming) return
  emit('sendMessage')
}

function uploadIcon(extension: string) {
  if (extension === 'pdf') return 'picture_as_pdf'
  if (['png', 'jpg', 'jpeg', 'gif', 'bmp', 'webp'].includes(extension)) return 'image'
  if (['doc', 'docx'].includes(extension)) return 'description'
  if (['ppt', 'pptx'].includes(extension)) return 'slideshow'
  if (['xls', 'xlsx', 'xlsm'].includes(extension)) return 'table_chart'
  return 'insert_drive_file'
}

function formatBytes(bytes: number) {
  if (!Number.isFinite(bytes) || bytes <= 0) return '0 B'
  const units = ['B', 'KB', 'MB', 'GB']
  let value = bytes
  let unitIndex = 0
  while (value >= 1024 && unitIndex < units.length - 1) {
    value /= 1024
    unitIndex += 1
  }
  return `${value.toFixed(value >= 10 || unitIndex === 0 ? 0 : 1)} ${units[unitIndex]}`
}

watch(
  () => props.disabled,
  (newDisabled) => {
    if (!newDisabled && composerInputRef.value) {
      setTimeout(() => {
        composerInputRef.value?.focus()
      }, 50)
    }
  }
)
</script>

<style scoped>
.chat-input-wrapper {
  width: 100%;
  display: flex;
  flex-direction: column;
  align-items: center;
}

.upload-strip {
  width: 100%;
  max-width: 900px;
  margin: 0 0 10px;
  border-radius: 16px;
  border: 1px solid #e7efe8;
  background: #f8fcf9;
  padding: 10px 12px;
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.upload-strip-title {
  font-size: 12px;
  color: #587263;
  font-weight: 700;
}

.upload-list {
  display: flex;
  gap: 8px;
  overflow-x: auto;
  padding-bottom: 2px;
}

.upload-chip {
  flex-shrink: 0;
  border-radius: 999px;
  border: 1px solid #d8e8dd;
  background: #ffffff;
  padding: 4px 8px;
  display: inline-flex;
  align-items: center;
  gap: 6px;
  max-width: 340px;
}

.upload-chip .material-icons-round {
  font-size: 16px;
  color: #4a7c59;
}

.upload-chip-name {
  font-size: 12px;
  color: #334155;
  max-width: 160px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.upload-chip-meta {
  font-size: 11px;
  color: #94a3b8;
}

.upload-chip-remove {
  width: 20px;
  height: 20px;
  border: none;
  border-radius: 999px;
  background: #eef4ef;
  color: #64806f;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
}

.upload-chip-remove .material-icons-round {
  font-size: 14px;
}

.input-bar {
  width: 100%;
  max-width: 900px;
  margin: 0 0 32px;
  flex-shrink: 0;
  border-radius: 999px;
  background: #FFFFFF;
  border: 1px solid #f1ece2;
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 4px 4px 4px 14px;
  box-shadow: 0 12px 40px rgba(0, 0, 0, 0.06);
  z-index: 5;
}

.input-bar input {
  flex: 1;
  border: none;
  outline: none;
  font-size: 13px;
  background: transparent;
  color: #44403c;
}

.input-bar input::placeholder {
  color: #a8a29e;
}

.model-selector {
  position: relative;
  border-right: 1px solid #e7e3db;
  padding-right: 8px;
  margin-right: 2px;
}

.model-trigger {
  height: 32px;
  border: none;
  border-radius: 999px;
  background: #f7f5f1;
  color: #78716c;
  display: flex;
  align-items: center;
  gap: 6px;
  padding: 0 10px;
  font-size: 11px;
  font-weight: 700;
  cursor: pointer;
  max-width: 124px;
}

.model-trigger:disabled {
  opacity: 0.55;
  cursor: not-allowed;
}

.model-trigger .material-icons-round {
  font-size: 16px;
  color: #a8a29e;
}

.model-dot {
  width: 7px;
  height: 7px;
  border-radius: 999px;
  background: #4a7c59;
  flex-shrink: 0;
  box-shadow: 0 0 0 3px rgba(74, 124, 89, 0.14);
}

.model-text {
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.model-dropdown {
  position: absolute;
  left: 0;
  bottom: calc(100% + 10px);
  width: 176px;
  border-radius: 12px;
  border: 1px solid #ece8df;
  background: rgba(255, 255, 255, 0.96);
  backdrop-filter: blur(8px);
  box-shadow: 0 18px 26px -18px rgba(0, 0, 0, 0.4);
  opacity: 0;
  visibility: hidden;
  transform: translateY(8px);
  transition: opacity 0.2s ease, transform 0.2s ease, visibility 0.2s ease;
  overflow: hidden;
  z-index: 70;
}

.model-selector:hover .model-dropdown,
.model-selector:focus-within .model-dropdown {
  opacity: 1;
  visibility: visible;
  transform: translateY(0);
}

.model-dropdown-title {
  font-size: 10px;
  font-weight: 800;
  color: #a8a29e;
  padding: 8px 10px 7px;
  border-bottom: 1px solid #f2ede6;
  letter-spacing: 0.08em;
}

.model-option {
  width: 100%;
  border: none;
  background: transparent;
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 8px 10px;
  font-size: 12px;
  font-weight: 700;
  color: #57534e;
  cursor: pointer;
}

.model-option .material-icons-round {
  font-size: 14px;
}

.model-option:hover {
  background: #f7f5f1;
}

.model-option.active {
  color: #4a7c59;
}

.attach-btn {
  border: none;
  width: 36px;
  height: 36px;
  border-radius: 999px;
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
}

.attach-btn {
  background: transparent;
  color: #9c958d;
}



.attach-btn .material-icons-round {
  font-size: 20px;
  transform: rotate(-45deg);
  transition: transform 0.2s ease, color 0.2s ease;
}



.attach-btn:hover .material-icons-round {
  transform: rotate(0deg);
  color: #4a7c59;
}

.send-btn {
  background: #4a7c59;
  color: #fff;
  width: 36px;
  height: 48px;
  border-radius: 18px;
  margin-left: 4px;
  border: none;
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  box-shadow: 0 4px 10px -2px rgba(74, 124, 89, 0.45);
  transition: transform 0.15s ease, background-color 0.2s ease, box-shadow 0.15s ease;
}

.send-btn:hover:not(:disabled) {
  background: #629672;
  transform: translateY(-1px);
  box-shadow: 0 6px 14px -2px rgba(74, 124, 89, 0.5);
}

.send-btn:disabled {
  opacity: 0.55;
  cursor: not-allowed;
  transform: none;
  box-shadow: none;
}

.send-btn .material-icons-round {
  font-size: 20px;
}

.send-stop-square {
  display: block;
  width: 14px;
  height: 14px;
  border-radius: 3px;
  background: #ffffff;
  flex-shrink: 0;
}

@media (max-width: 900px) {
  .upload-strip {
    margin: 0 14px 8px;
  }
}
</style>
