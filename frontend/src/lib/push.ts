import { api } from './api'

function urlBase64ToUint8Array(base64String: string) {
  const padding = '='.repeat((4 - (base64String.length % 4)) % 4)
  const base64 = (base64String + padding).replace(/-/g, '+').replace(/_/g, '/')
  const raw = atob(base64)
  const output = new Uint8Array(raw.length)
  for (let i = 0; i < raw.length; i++) output[i] = raw.charCodeAt(i)
  return output
}

export async function enableNotifications() {
  if (!('serviceWorker' in navigator) || !('PushManager' in window)) {
    throw new Error('Push notifications are not supported in this browser')
  }
  const permission = await Notification.requestPermission()
  if (permission !== 'granted') throw new Error('Notification permission denied')

  const reg = await navigator.serviceWorker.register('/sw.js')
  const { vapid_public_key } = await api.ensureVapid()
  const sub = await reg.pushManager.subscribe({
    userVisibleOnly: true,
    applicationServerKey: urlBase64ToUint8Array(vapid_public_key),
  })
  await api.subscribePush(sub.toJSON())
}
