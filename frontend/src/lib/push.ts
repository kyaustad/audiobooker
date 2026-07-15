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
    return { supported: true, permission, subscribed: server.subscribed }
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

  const reg = await navigator.serviceWorker.register('/sw.js')
  await navigator.serviceWorker.ready

  const { vapid_public_key } = await api.ensureVapid()
  let sub = await reg.pushManager.getSubscription()
  if (!sub) {
    sub = await reg.pushManager.subscribe({
      userVisibleOnly: true,
      applicationServerKey: urlBase64ToUint8Array(vapid_public_key),
    })
  }
  await api.subscribePush(sub.toJSON())
  return sub
}

export async function disableNotifications() {
  if (!('serviceWorker' in navigator)) return
  const reg = await navigator.serviceWorker.getRegistration('/sw.js')
  const sub = await reg?.pushManager.getSubscription()
  if (sub) {
    await api.unsubscribePush(sub.endpoint)
    await sub.unsubscribe()
  } else {
    await api.unsubscribePush()
  }
}
