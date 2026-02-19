import { createRouter, createWebHistory, RouteRecordRaw } from 'vue-router'
import { invoke } from '@tauri-apps/api/core'
import HomeWorkspace from '../HomeWorkspace.vue'
import SettingsView from '../views/SettingsView.vue'
import AccountView from '../views/AccountView.vue'
import LoginView from '../views/LoginView.vue'

const routes: RouteRecordRaw[] = [
  {
    path: '/login',
    name: 'login',
    component: LoginView,
    meta: { public: true },  // 不需要登录
  },
  {
    path: '/',
    name: 'home',
    component: HomeWorkspace
  },
  {
    path: '/settings',
    redirect: '/settings/general'
  },
  {
    path: '/settings/:section(general|notifications|about|advanced)',
    name: 'settings',
    component: SettingsView
  },
  {
    path: '/settings/about/:page(feedback|tutorial|agreement)',
    name: 'settings-about-page',
    component: SettingsView
  },
  {
    path: '/account',
    redirect: '/account/profile'
  },
  {
    path: '/account/:section(profile|renew|orders|quota)',
    name: 'account',
    component: AccountView
  },
  {
    path: '/:pathMatch(.*)*',
    redirect: '/'
  }
]

const router = createRouter({
  history: createWebHistory(),
  routes
})

// 路由守卫：未登录跳转到 /login，已登录访问 /login 跳转回首页
router.beforeEach(async (to) => {
  if (to.meta.public) {
    // 已登录访问登录页 → 直接回首页
    try {
      const loggedIn = await invoke<boolean>('petool_is_logged_in')
      if (loggedIn) return '/'
    } catch {
      // ignore
    }
    return true
  }

  // 其他页面需要登录
  try {
    const loggedIn = await invoke<boolean>('petool_is_logged_in')
    if (!loggedIn) return '/login'
  } catch {
    return '/login'
  }
  return true
})

export default router

