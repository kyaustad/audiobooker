/** Register SW and reload when a new version activates (installed PWAs). */
export function registerServiceWorker() {
  if (!('serviceWorker' in navigator)) return

  const reloadOnce = () => {
    const key = 'ab-sw-reload'
    if (sessionStorage.getItem(key)) return
    sessionStorage.setItem(key, '1')
    location.reload()
  }

  navigator.serviceWorker.addEventListener('controllerchange', () => {
    reloadOnce()
  })

  navigator.serviceWorker.addEventListener('message', (event) => {
    if (event.data?.type === 'SW_ACTIVATED') reloadOnce()
    if (event.data?.type === 'NOTIFICATION_NAV' && typeof event.data.url === 'string') {
      const url = event.data.url as string
      if (url.startsWith('/#') || url.startsWith('#')) {
        location.hash = url.replace(/^\/#/, '#').replace(/^#/, '#')
      } else if (url.startsWith('/')) {
        location.assign(url)
      }
    }
  })

  const register = async () => {
    try {
      const reg = await navigator.serviceWorker.register('/sw.js', { updateViaCache: 'none' })
      const ping = () => reg.update().catch(() => undefined)
      ping()
      document.addEventListener('visibilitychange', () => {
        if (document.visibilityState === 'visible') ping()
      })
      window.setInterval(ping, 5 * 60 * 1000)
    } catch {
      /* ignore */
    }
  }

  if (document.readyState === 'complete') register()
  else window.addEventListener('load', register)
}
