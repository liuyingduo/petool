import { createRouter, createWebHistory, RouteRecordRaw } from 'vue-router'
import HomeWorkspace from '../HomeWorkspace.vue'
import SettingsView from '../views/SettingsView.vue'
import AccountView from '../views/AccountView.vue'

const routes: RouteRecordRaw[] = [
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

export default router
