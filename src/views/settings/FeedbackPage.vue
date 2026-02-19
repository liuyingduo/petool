<template>
  <div class="settings-stack">
    <div class="settings-card">
      <h3 class="setting-title">给 Petool 的信</h3>
      <p class="setting-desc">你的每一个反馈，都在帮助 Petool 变得更聪明。</p>

      <div style="margin-top: 14px; display: flex; flex-direction: column; gap: 14px;">
        <div>
          <p class="settings-section-title" style="font-size: 13px;">请选择反馈类型</p>
          <div class="feedback-grid" style="margin-top: 8px;">
            <div>
              <input id="feedback-bug" v-model="form.category" name="feedback-category" type="radio" value="bug" />
              <label class="feedback-chip" for="feedback-bug">遇到 Bug</label>
            </div>
            <div>
              <input id="feedback-idea" v-model="form.category" name="feedback-category" type="radio" value="idea" />
              <label class="feedback-chip" for="feedback-idea">新点子</label>
            </div>
            <div>
              <input id="feedback-question" v-model="form.category" name="feedback-category" type="radio" value="question" />
              <label class="feedback-chip" for="feedback-question">咨询</label>
            </div>
          </div>
        </div>

        <div>
          <p class="settings-section-title" style="font-size: 13px;">详细描述</p>
          <textarea v-model="form.detail" class="field-textarea" placeholder="描述遇到的问题或建议..."></textarea>
        </div>

        <div>
          <p class="settings-section-title" style="font-size: 13px;">附件</p>
          <button class="attach-dropzone" type="button" @click="pickAttachments">
            <span class="material-icons-round" style="color: #a8a29e;">add</span>
            <span style="font-size: 12px; color: #78716c; font-weight: 700;">点击上传截图或日志文件</span>
          </button>
          <div v-if="form.attachments.length > 0" class="file-list">
            <div v-for="file in form.attachments" :key="file" class="file-chip" :title="file">
              <span class="material-icons-round" style="font-size: 16px;">attach_file</span>
              <span class="name">{{ file }}</span>
            </div>
          </div>
        </div>

        <div class="path-grid" style="margin-top: 0; grid-template-columns: 1fr auto;">
          <input v-model="form.email" class="field-input" placeholder="联系邮箱（可选）" type="email" />
          <button class="btn primary" :disabled="submitting" type="button" @click="submit">
            {{ submitting ? '提交中...' : '提交反馈' }}
          </button>
        </div>
      </div>

      <div v-if="status.text" class="status-chip" :class="status.type">{{ status.text }}</div>
      <div v-if="savedPath" class="status-chip info">草稿已保存：{{ savedPath }}</div>
    </div>
  </div>
</template>

<script setup lang="ts">
import { reactive, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'
import { open as openDialog } from '@tauri-apps/plugin-dialog'

const submitting = ref(false)
const savedPath = ref('')
const status = ref<{ type: 'success' | 'error' | 'info'; text: string }>({ type: 'info', text: '' })

const form = reactive({
  category: 'bug',
  detail: '',
  email: '',
  attachments: [] as string[]
})

async function pickAttachments() {
  try {
    const selected = await openDialog({
      title: '选择附件',
      multiple: true,
      directory: false
    })
    if (!selected) return
    const paths = Array.isArray(selected) ? selected : [selected]
    const unique = paths.filter((item, index) => item && paths.indexOf(item) === index)
    form.attachments = [...form.attachments, ...unique.filter((item) => !form.attachments.includes(item))]
  } catch {
    status.value = { type: 'error', text: '选择附件失败。' }
  }
}

async function submit() {
  if (!form.detail.trim()) {
    status.value = { type: 'error', text: '请填写详细描述。' }
    return
  }

  submitting.value = true
  status.value = { type: 'info', text: '' }
  savedPath.value = ''
  try {
    const result = await invoke<{ saved_json_path: string }>('submit_feedback', {
      input: {
        category: form.category,
        detail: form.detail,
        email: form.email,
        attachments: form.attachments,
        client_meta: {
          platform: navigator.platform,
          user_agent: navigator.userAgent
        }
      }
    })
    savedPath.value = result.saved_json_path
    status.value = { type: 'success', text: '反馈已保存到本地草稿。' }
  } catch (error) {
    status.value = { type: 'error', text: typeof error === 'string' ? error : '反馈提交失败。' }
  } finally {
    submitting.value = false
  }
}
</script>
