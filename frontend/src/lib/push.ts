import { api, type NotificationPrefs } from './api'

function urlBase64ToUint8Array(base64String: string) {
  const padding = '='.repeat((4 - (base64String.length % 4)) % 4)
  const base64 = (base64String + padding).replace(/-/g, '+').replace(/_/g, '/')
  const raw = atob(base64)
  const output = new Uint8Array(raw.length)
  for (let i = 0; i < raw.length; i++) output[i] = raw.charCodeAt(i)
  return output
}

export function isIosDevice() {
  if (typeof navigator === 'undefined') return false
  const ua = navigator.userAgent || ''
  const iOS = /iPad|iPhone|iPod/.test(ua)
  const iPadOs = navigator.platform === 'MacIntel' && navigator.maxTouchPoints > 1
  return iOS || iPadOs
}

export function isStandalonePwa() {
  if (typeof window === 'undefined') return false
  const mq = window.matchMedia?.('(display-mode: standalone)')?.matches
  const legacy = 'standalone' in navigator && Boolean((navigator as Navigator & { standalone?: boolean }).standalone)
  return Boolean(mq || legacy)
}

export type PushCapability = {
  supported: boolean
  permission: NotificationPermission | 'denied' | 'default' | 'granted'
  subscribed: boolean
  needsHttps: boolean
  needsInstall: boolean
  isIos: boolean
  preferences: NotificationPrefs
}

const defaultPrefs = (): NotificationPrefs => ({
  notify_imported: true,
  notify_download_finished: false,
  notify_pack_ready: true,
  notify_failures: true,
})

export async function getPushStatus(): Promise<PushCapability> {
  const needsHttps = !window.isSecureContext && location.hostname !== 'localhost'
  const ios = isIosDevice()
  const standalone = isStandalonePwa()
  const hasPush = 'serviceWorker' in navigator && 'PushManager' in window
  const needsInstall = ios && !standalone

  if (!hasPush) {
    return {
      supported: false,
      permission: 'denied',
      subscribed: false,
      needsHttps,
      needsInstall,
      isIos: ios,
      preferences: defaultPrefs(),
    }
  }

  const permission = Notification.permission
  try {
    const server = await api.pushStatus()
    return {
      supported: true,
      permission,
      subscribed: server.subscribed && permission === 'granted',
      needsHttps,
      needsInstall: false,
      isIos: ios,
      preferences: server.preferences ?? defaultPrefs(),
    }
  } catch {
    return {
      supported: true,
      permission,
      subscribed: false,
      needsHttps,
      needsInstall: false,
      isIos: ios,
      preferences: defaultPrefs(),
    }
  }
}

export async function enableNotifications() {
  if (!('serviceWorker' in navigator) || !('PushManager' in window)) {
    if (isIosDevice() && !isStandalonePwa()) {
      throw new Error(
        'On iPhone/iPad, install Audiobooker to your Home Screen first, then open it from the icon and enable notifications.',
      )
    }
    throw new Error('Push notifications are not supported in this browser')
  }
  if (!window.isSecureContext && location.hostname !== 'localhost') {
    throw new Error('Notifications require HTTPS (or localhost)')
  }

  const permission = await Notification.requestPermission()
  if (permission !== 'granted') throw new Error('Notification permission denied')

  const reg = await navigator.serviceWorker.register('/sw.js', { scope: '/', updateViaCache: 'none' })
  await navigator.serviceWorker.ready

  const { vapid_public_key } = await api.ensureVapid()
  const key = urlBase64ToUint8Array(vapid_public_key)

  let sub = await reg.pushManager.getSubscription()
  if (sub) {
    try {
      await api.subscribePush(sub.toJSON())
      return sub
    } catch {
      await sub.unsubscribe().catch(() => undefined)
      sub = null
    }
  }

  sub = await reg.pushManager.subscribe({
    userVisibleOnly: true,
    applicationServerKey: key,
  })
  await api.subscribePush(sub.toJSON())
  return sub
}

export async function disableNotifications() {
  if (!('serviceWorker' in navigator)) return
  const reg = await navigator.serviceWorker.getRegistration('/')
  const sub = await reg?.pushManager.getSubscription()
  if (sub) {
    await api.unsubscribePush(sub.endpoint)
    await sub.unsubscribe()
  } else {
    await api.unsubscribePush()
  }
}

export async function sendTestNotification() {
  await api.testPush()
}

export async function saveNotificationPrefs(prefs: NotificationPrefs) {
  await api.updatePushPreferences(prefs)
}
