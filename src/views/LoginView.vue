<template>
  <div class="login-shell">
    <div class="login-bg" aria-hidden="true">
      <div class="login-blob login-blob-a"></div>
      <div class="login-blob login-blob-b"></div>
    </div>

    <!-- 窗口控制按钮 -->
    <div class="login-window-controls">
      <button class="login-window-btn" type="button" title="最小化" @click="handleMinimize">
        <span class="material-icons-round">remove</span>
      </button>
      <button class="login-window-btn" type="button" :title="isMaximized ? '还原' : '最大化'" @click="handleToggleMaximize">
        <span class="material-icons-round">{{ isMaximized ? 'filter_none' : 'check_box_outline_blank' }}</span>
      </button>
      <button class="login-window-btn close" type="button" title="关闭" @click="handleClose">
        <span class="material-icons-round">close</span>
      </button>
    </div>

    <div class="login-card">
      <!-- Logo -->
      <div class="login-logo">
        <span class="material-icons-round" style="font-size:36px; color: var(--accent)">auto_awesome</span>
        <h1>Petool</h1>
      </div>

      <!-- Tab 切换 -->
      <div class="login-tabs">
        <button
          class="login-tab"
          :class="{ active: mode === 'login' }"
          type="button"
          @click="mode = 'login'; error = ''"
        >登录</button>
        <button
          class="login-tab"
          :class="{ active: mode === 'register' }"
          type="button"
          @click="mode = 'register'; error = ''"
        >注册</button>
      </div>

      <!-- 错误提示 -->
      <div v-if="error" class="login-error">
        <span class="material-icons-round" style="font-size:16px">error_outline</span>
        {{ error }}
      </div>

      <!-- 成功提示 -->
      <div v-if="success" class="login-success">
        <span class="material-icons-round" style="font-size:16px">check_circle</span>
        {{ success }}
      </div>

      <!-- 登录表单 -->
      <form v-if="mode === 'login'" class="login-form" @submit.prevent="handleLogin">
        <div class="login-field">
          <label>邮箱</label>
          <input v-model="email" type="email" placeholder="your@email.com" required autocomplete="email" />
        </div>
        <div class="login-field">
          <label>密码</label>
          <input v-model="password" type="password" placeholder="输入密码" required autocomplete="current-password" />
        </div>
        <button class="login-submit" type="submit" :disabled="loading">
          <span v-if="loading" class="material-icons-round spin">sync</span>
          <span v-else>登录</span>
        </button>
      </form>

      <!-- 注册表单 -->
      <form v-else class="login-form" @submit.prevent="handleRegister">
        <div class="login-field">
          <label>用户名</label>
          <input v-model="username" type="text" placeholder="2-20 个字符" required autocomplete="username" />
        </div>
        <div class="login-field">
          <label>邮箱</label>
          <input v-model="email" type="email" placeholder="your@email.com" required autocomplete="email" />
        </div>
        <div class="login-field">
          <label>密码</label>
          <input v-model="password" type="password" placeholder="至少 8 位" required autocomplete="new-password" />
        </div>
        <button class="login-submit" type="submit" :disabled="loading">
          <span v-if="loading" class="material-icons-round spin">sync</span>
          <span v-else>注册并登录</span>
        </button>
      </form>
    </div>
  </div>
</template>

<script setup lang="ts">
import { ref, onMounted } from 'vue'
import { useRouter } from 'vue-router'
import { invoke } from '@tauri-apps/api/core'
import { getCurrentWindow } from '@tauri-apps/api/window'

const router = useRouter()
const appWindow = getCurrentWindow()

const mode = ref<'login' | 'register'>('login')
const email = ref('')
const password = ref('')
const username = ref('')
const loading = ref(false)
const error = ref('')
const success = ref('')
const isMaximized = ref(false)

onMounted(async () => {
  isMaximized.value = await appWindow.isMaximized()
})

async function handleMinimize() {
  await appWindow.minimize()
}

async function handleToggleMaximize() {
  await appWindow.toggleMaximize()
  isMaximized.value = await appWindow.isMaximized()
}

async function handleClose() {
  try { await invoke('app_exit_now') } catch { await appWindow.close() }
}

async function handleLogin() {
  loading.value = true
  error.value = ''
  try {
    await invoke('petool_login', { email: email.value, password: password.value })
    await router.replace('/')
  } catch (e: any) {
    error.value = e?.toString() || '登录失败，请检查邮箱和密码'
  } finally {
    loading.value = false
  }
}

async function handleRegister() {
  loading.value = true
  error.value = ''
  success.value = ''
  if (password.value.length < 8) {
    error.value = '密码至少需要 8 位'
    loading.value = false
    return
  }
  try {
    await invoke('petool_register', {
      username: username.value,
      email: email.value,
      password: password.value,
    })
    success.value = '注册成功，赠送 5 万 Token！正在进入...'
    setTimeout(() => { void router.replace('/') }, 1000)
  } catch (e: any) {
    error.value = e?.toString() || '注册失败，请重试'
  } finally {
    loading.value = false
  }
}
</script>

<style scoped>
.login-shell {
  width: 100vw; height: 100vh;
  display: flex; align-items: center; justify-content: center;
  position: relative; overflow: hidden;
  background: var(--bg-primary, #0f0f1a);
  /* 允许拖动窗口 */
  -webkit-app-region: drag;
}
/* 卡片内不拖动 */
.login-card, button, input { -webkit-app-region: no-drag; }

.login-window-controls {
  position: fixed;
  top: 0; right: 0;
  display: flex;
  z-index: 100;
  -webkit-app-region: no-drag;
}
.login-window-btn {
  width: 46px; height: 32px;
  border: none; background: transparent;
  color: rgba(255,255,255,0.5);
  cursor: pointer; display: flex; align-items: center; justify-content: center;
  transition: background 0.15s, color 0.15s;
  font-size: 16px;
}
.login-window-btn:hover { background: rgba(255,255,255,0.1); color: #fff; }
.login-window-btn.close:hover { background: #ef4444; color: #fff; }
.login-window-btn .material-icons-round { font-size: 16px; }

.login-bg { position: absolute; inset: 0; pointer-events: none; }
.login-blob {
  position: absolute; border-radius: 50%;
  filter: blur(80px); opacity: 0.25;
}
.login-blob-a {
  width: 400px; height: 400px;
  background: radial-gradient(circle, #8b5cf6, transparent);
  top: -100px; left: -100px;
}
.login-blob-b {
  width: 300px; height: 300px;
  background: radial-gradient(circle, #06b6d4, transparent);
  bottom: -80px; right: -80px;
}

.login-card {
  position: relative; z-index: 1;
  background: rgba(255,255,255,0.04);
  border: 1px solid rgba(255,255,255,0.08);
  border-radius: 20px;
  padding: 36px 32px;
  width: 360px;
  backdrop-filter: blur(12px);
  box-shadow: 0 24px 64px rgba(0,0,0,0.4);
}

.login-logo {
  display: flex; align-items: center; gap: 10px;
  margin-bottom: 24px;
}
.login-logo h1 {
  margin: 0; font-size: 22px; font-weight: 700;
  background: linear-gradient(135deg, #8b5cf6, #06b6d4);
  -webkit-background-clip: text; -webkit-text-fill-color: transparent;
}

.login-tabs {
  display: flex; gap: 4px;
  background: rgba(255,255,255,0.05);
  border-radius: 10px; padding: 4px;
  margin-bottom: 20px;
}
.login-tab {
  flex: 1; padding: 8px; border-radius: 8px;
  border: none; background: transparent;
  color: var(--text-secondary, #94a3b8);
  cursor: pointer; font-size: 14px; font-weight: 500;
  transition: all 0.2s;
}
.login-tab.active {
  background: rgba(139,92,246,0.2);
  color: #c4b5fd;
}

.login-error {
  display: flex; align-items: center; gap: 6px;
  background: rgba(239,68,68,0.12);
  border: 1px solid rgba(239,68,68,0.3);
  border-radius: 8px; padding: 10px 12px;
  color: #fca5a5; font-size: 13px; margin-bottom: 12px;
}

.login-success {
  display: flex; align-items: center; gap: 6px;
  background: rgba(34,197,94,0.12);
  border: 1px solid rgba(34,197,94,0.3);
  border-radius: 8px; padding: 10px 12px;
  color: #86efac; font-size: 13px; margin-bottom: 12px;
}

.login-form { display: flex; flex-direction: column; gap: 14px; }

.login-field { display: flex; flex-direction: column; gap: 6px; }
.login-field label { font-size: 13px; color: var(--text-secondary, #94a3b8); }
.login-field input {
  padding: 10px 14px;
  background: rgba(255,255,255,0.06);
  border: 1px solid rgba(255,255,255,0.1);
  border-radius: 10px; color: inherit;
  font-size: 14px; outline: none;
  transition: border-color 0.2s;
}
.login-field input:focus {
  border-color: rgba(139,92,246,0.6);
  box-shadow: 0 0 0 3px rgba(139,92,246,0.1);
}

.login-submit {
  margin-top: 4px; padding: 12px;
  background: linear-gradient(135deg, #8b5cf6, #7c3aed);
  border: none; border-radius: 10px;
  color: white; font-size: 15px; font-weight: 600;
  cursor: pointer; transition: opacity 0.2s;
  display: flex; align-items: center; justify-content: center; gap: 8px;
}
.login-submit:hover:not(:disabled) { opacity: 0.9; }
.login-submit:disabled { opacity: 0.6; cursor: not-allowed; }

.spin { animation: spin 1s linear infinite; }
@keyframes spin { to { transform: rotate(360deg); } }
</style>
