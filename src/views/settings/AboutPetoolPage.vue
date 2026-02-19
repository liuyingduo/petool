<template>
  <div class="settings-stack">
    <div class="settings-card about-hero">
      <h2 class="about-title">Petool</h2>
      <div class="version-pill">
        <span>v2.4.0</span>
        <span>Build 20231024</span>
      </div>
      <p class="setting-desc" style="margin-top: 14px; font-size: 15px;">
        Petool 是你的个人智能伙伴，致力于通过高效 AI 交互，让每一项任务都更简单有趣。
      </p>
    </div>

    <div class="settings-card compact">
      <h3 class="settings-section-title">帮助与信息</h3>
      <p class="settings-section-desc">查看教程、协议，或提交反馈问题。</p>

      <div class="about-grid">
        <button class="about-link" type="button" @click="emit('open-page', 'tutorial')">
          <span class="about-link-icon blue"><span class="material-icons-round">menu_book</span></span>
          <span>使用教程</span>
        </button>
        <button class="about-link" type="button" @click="emit('open-page', 'agreement')">
          <span class="about-link-icon purple"><span class="material-icons-round">description</span></span>
          <span>用户协议</span>
        </button>
        <button class="about-link" type="button" @click="emit('open-page', 'feedback')">
          <span class="about-link-icon orange"><span class="material-icons-round">feedback</span></span>
          <span>反馈问题</span>
        </button>
      </div>
    </div>

    <div class="meta-footer">© 2026 Petool Team. All Rights Reserved.</div>
  </div>
</template>

<script setup lang="ts">
const emit = defineEmits<{
  (event: 'open-page', page: 'feedback' | 'tutorial' | 'agreement'): void
}>()
</script>
