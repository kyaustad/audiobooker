import { api } from './api'

function urlBase64ToUint8Array(base64String: string) {
  const padding = '='.repeat((4 - (base64String.length % 4)) % 4)
  const base64 = (base64String + padding).replace(/-/g, '+').replace(/_/g, '/')
  const raw = atob(base64)
  const output = new Uint8Array(raw.length)
  for (let i = 0; i < raw.length; i++) output[i] = raw.charCodeAt(i)
  return output
}

export async function getPushStatus() {
  if (!('serviceWorker' in navigator) || !('PushManager' in window)) {
    return { supported: false, permission: 'denied' as NotificationPermission | 'denied', subscribed: false }
  }
  const permission = Notification.permission
  try {
    const server = await api.pushStatus()
    return { supported: true, permission, subscribed: server.subscribed && permission === 'granted' }
  } catch {
    return { supported: true, permission, subscribed: false }
  }
}

export async function enableNotifications() {
  if (!('serviceWorker' in navigator) || !('PushManager' in window)) {
    throw new Error('Push notifications are not supported in this browser')
  }
  if (!window.isSecureContext && location.hostname !== 'localhost') {
    throw new Error('Notifications require HTTPS (or localhost)')
  }

  const permission = await Notification.requestPermission()
  if (permission !== 'granted') throw new Error('Notification permission denied')

  const reg = await navigator.serviceWorker.register('/sw.js', { scope: '/' })
  await navigator.serviceWorker.ready

  const { vapid_public_key } = await api.ensureVapid()
  const key = urlBase64ToUint8Array(vapid_public_key)

  let sub = await reg.pushManager.getSubscription()
  if (sub) {
    // Re-subscribe if the browser subscription is stale vs current VAPID key.
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
