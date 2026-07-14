<script lang="ts">
  import Router, { push, router } from 'svelte-spa-router'
  import { onMount } from 'svelte'
  import { api } from './lib/api'
  import { currentUser } from './lib/session'
  import { toast } from './lib/toast'
  import Setup from './pages/Setup.svelte'
  import Login from './pages/Login.svelte'
  import Dashboard from './pages/Dashboard.svelte'
  import Match from './pages/Match.svelte'
  import Settings from './pages/Settings.svelte'
  import Users from './pages/Users.svelte'
  import ApiKey from './pages/ApiKey.svelte'
  import Browse from './pages/Browse.svelte'
  import ChangePassword from './pages/ChangePassword.svelte'

  let loading = $state(true)
  let toastMsg = $state<string | null>(null)
  let user = $state(null as import('./lib/api').User | null)
  let path = $state(router.location)

  currentUser.subscribe((v) => (user = v))
  toast.subscribe((v) => (toastMsg = v))

  // Poll hash location for nav highlighting (router.location is not a Svelte store in v5)
  onMount(() => {
    const sync = () => {
      path = router.location
    }
    sync()
    window.addEventListener('hashchange', sync)
    return () => window.removeEventListener('hashchange', sync)
  })

  const authedRoutes = {
    '/': Dashboard,
    '/match/:id': Match,
    '/settings': Settings,
    '/users': Users,
    '/api-key': ApiKey,
    '/browse': Browse,
    '/password': ChangePassword,
  }

  const publicRoutes = {
    '/setup': Setup,
    '/login': Login,
    '*': Login,
  }

  async function bootstrap() {
    try {
      const status = await api.setupStatus()
      if (status.needs_setup) {
        await push('/setup')
        return
      }
      try {
        const me = await api.me()
        currentUser.set(me.user)
        if (me.user.must_change_password) await push('/password')
        else if (me.user.role === 'root' && (path === '/' || path === '/login')) {
          await push('/settings')
        } else if (path === '/login' || path === '/setup') {
          await push(me.user.role === 'root' ? '/settings' : '/')
        }
      } catch {
        if (path !== '/login' && path !== '/setup') await push('/login')
      }
    } finally {
      loading = false
      path = router.location
    }
  }

  onMount(() => {
    bootstrap()
    if ('serviceWorker' in navigator) {
      navigator.serviceWorker.register('/sw.js').catch(() => undefined)
    }
  })

  async function logout() {
    await api.logout()
    currentUser.set(null)
    await push('/login')
    path = router.location
  }
</script>

{#if loading}
  <div class="auth-wrap"><p class="muted">Loading Audiobooker…</p></div>
{:else if user && path !== '/login' && path !== '/setup'}
  <div class="shell">
    <header class="topbar">
      <div class="brand">Audiobooker</div>
      <nav class="nav">
        {#if user.role === 'user'}
          <a href="#/" class:active={path === '/'}>Queue</a>
          <a href="#/browse" class:active={path.startsWith('/browse')}>AudiobookBay</a>
        {/if}
        {#if user.role === 'root'}
          <a href="#/settings" class:active={path === '/settings'}>Settings</a>
          <a href="#/users" class:active={path === '/users'}>Users</a>
          <a href="#/api-key" class:active={path === '/api-key'}>API Key</a>
        {/if}
        <a href="#/password" class:active={path === '/password'}>Password</a>
        <button class="linkish" type="button" onclick={logout}>Sign out</button>
      </nav>
    </header>
    <Router routes={authedRoutes} />
  </div>
{:else}
  <Router routes={publicRoutes} />
{/if}

{#if toastMsg}
  <div class="toast">{toastMsg}</div>
{/if}
