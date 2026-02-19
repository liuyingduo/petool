<template>
  <div class="account-stack">
    <section class="account-section">
      <div class="section-title-row">
        <h2><span class="material-icons-round">card_membership</span>订阅会员</h2>
      </div>
      <div class="plan-grid">
        <article
          v-for="plan in renewPlans"
          :key="plan.id"
          class="account-card plan-card"
          :class="{ featured: plan.featured }"
        >
          <div v-if="plan.badge" class="plan-badge">{{ plan.badge }}</div>
          <div class="plan-head">
            <h3>{{ plan.name }}</h3>
            <div class="plan-price">
              <span>{{ plan.priceLabel }}</span>
              <small>{{ plan.priceUnit }}</small>
            </div>
            <div v-if="plan.originalPrice" class="plan-origin">{{ plan.originalPrice }}</div>
          </div>
          <ul class="plan-feature-list">
            <li v-for="feature in plan.features" :key="feature">
              <span class="material-icons-round">{{ plan.featured ? 'verified' : 'check_circle' }}</span>
              <span>{{ feature }}</span>
            </li>
          </ul>
          <button class="account-btn" :class="plan.featured ? 'primary' : 'secondary'" type="button">
            立即开通
          </button>
        </article>
      </div>
    </section>

    <section class="account-section">
      <div class="section-title-row">
        <h2><span class="material-icons-round">bolt</span>流量加油包</h2>
      </div>
      <div class="pack-grid">
        <article v-for="pack in tokenPacks" :key="pack.id" class="account-card pack-card" :class="{ unavailable: !pack.available }">
          <div>
            <div class="pack-name">{{ pack.name }}</div>
            <div class="pack-desc">{{ pack.desc }}</div>
            <div class="pack-price">{{ pack.priceLabel }}</div>
          </div>
          <button class="account-btn secondary" type="button" :disabled="!pack.available">
            {{ pack.available ? '立即购买' : '缺货' }}
          </button>
        </article>
      </div>
    </section>
  </div>
</template>

<script setup lang="ts">
import { renewPlans, tokenPacks } from './mock'
</script>
