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
    meta: { public: true }
  },
  {
    path: '/',
    name: 'home',
    component: HomeWorkspace,
    meta: { keepAlive: true }
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

// Route guard:
// 1) Unauthenticated users are redirected to /login.
// 2) Already-authenticated users visiting /login are redirected to /.
// 3) Protected -> protected route switches skip repeated auth IPC for speed.
router.beforeEach(async (to, from) => {
  const fromIsProtected = from.matched.length > 0 && from.matched.every((record) => !record.meta.public)
  const toIsProtected = to.matched.length > 0 && to.matched.every((record) => !record.meta.public)

  if (fromIsProtected && toIsProtected) {
    return true
  }

  if (to.meta.public) {
    try {
      const loggedIn = await invoke<boolean>('petool_is_logged_in')
      if (loggedIn) return '/'
    } catch {
      // ignore
    }
    return true
  }

  try {
    const loggedIn = await invoke<boolean>('petool_is_logged_in')
    if (!loggedIn) return '/login'
  } catch {
    return '/login'
  }

  return true
})

export default router
