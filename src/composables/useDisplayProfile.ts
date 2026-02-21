import { computed, ref } from 'vue'
import { invoke } from '@tauri-apps/api/core'

interface PetoolProfile {
  username: string
  avatar: string | null
  membership_level: string | null
}

const DEFAULT_AVATAR =
  'https://lh3.googleusercontent.com/aida-public/AB6AXuBYaZM97JogdW-ya3ULqGOtiyNOHmX7QgQJQ1c7qMdDxTpN__9ZBn0Jq6D5AQiHwClbXSmKaP3yFa-GzJuTHIsZ6OObIjCQ9QHApIpAuKMYIWptOHH6KVzLGp4nU5DO48mIg48o3YedtwFShv6G0Tq-ir30SVT7WgAWCksaPf_PnwnEwCx7rOimt23ZlQC3VUyfRbucQrEvpTkLIEwEwiWZ_gSWFyekl4IxXUqKEUqrS2CVHHlvuJqUmCJBLBYKUuDKiuQqkueqB3Y'

const profile = ref<PetoolProfile | null>(null)
const isLoggedIn = ref(false)
let loadingPromise: Promise<void> | null = null

function mapMembershipLevelToPlan(level: string | null | undefined) {
  if (level === 'pro') return 'Pro Plan'
  if (level === 'enterprise') return 'Enterprise'
  return 'Free Plan'
}

async function loadDisplayProfile(force = false) {
  if (loadingPromise && !force) return loadingPromise

  loadingPromise = (async () => {
    try {
      const loggedIn = await invoke<boolean>('petool_is_logged_in')
      isLoggedIn.value = loggedIn
      if (!loggedIn) {
        profile.value = null
        return
      }
      profile.value = await invoke<PetoolProfile>('petool_get_profile')
    } catch {
      // Keep fallback display values when profile is unavailable.
    } finally {
      loadingPromise = null
    }
  })()

  return loadingPromise
}

export function useDisplayProfile() {
  const displayName = computed(() => {
    const username = profile.value?.username
    if (typeof username === 'string' && username.trim()) {
      return username.trim()
    }
    return 'Alex'
  })

  const displayAvatar = computed(() => {
    const avatar = profile.value?.avatar
    if (typeof avatar === 'string' && avatar.trim()) {
      return avatar.trim()
    }
    return DEFAULT_AVATAR
  })

  const displayPlan = computed(() => {
    return mapMembershipLevelToPlan(profile.value?.membership_level)
  })

  return {
    isLoggedIn,
    rawProfile: profile,
    displayName,
    displayAvatar,
    displayPlan,
    loadDisplayProfile
  }
}
