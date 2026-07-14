import { writable } from 'svelte/store'

export const toast = writable<string | null>(null)

export function showToast(message: string, ms = 3500) {
  toast.set(message)
  window.setTimeout(() => toast.set(null), ms)
}
