<script lang="ts">
  import Router, { push, router } from 'svelte-spa-router'
  import { onMount } from 'svelte'
  import { api } from './lib/api'
  import { currentUser } from './lib/session'
  import { toast } from './lib/toast'
  import { registerServiceWorker } from './lib/sw'
  import Setup from './pages/Setup.svelte'
  import Login from './pages/Login.svelte'
  import Dashboard from './pages/Dashboard.svelte'
  import Match from './pages/Match.svelte'
  import Settings from './pages/Settings.svelte'
  import Users from './pages/Users.svelte'
  import ApiKey from './pages/ApiKey.svelte'
  import Browse from './pages/Browse.svelte'
  import ChangePassword from './pages/ChangePassword.svelte'
  import MapPack from './pages/MapPack.svelte'
  import Approvals from './pages/Approvals.svelte'
  import CreatedBy from './lib/CreatedBy.svelte'
  import BottomNavIcon from './lib/BottomNavIcon.svelte'
  import { canApprove, isOperator, isRoot } from './lib/roles'

  let loading = $state(true)
  let toastMsg = $state<string | null>(null)
  let user = $state(null as import('./lib/api').User | null)
  let path = $state(router.location)
  let menuOpen = $state(false)

  currentUser.subscribe((v) => (user = v))
  toast.subscribe((v) => (toastMsg = v))

  onMount(() => {
    const sync = () => {
      path = router.location
      menuOpen = false
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
    '/map/:id': MapPack,
    '/approvals': Approvals,
    '/password': ChangePassword,
    '/account': ChangePassword,
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
    registerServiceWorker()
  })

  async function logout() {
    menuOpen = false
    await api.logout()
    currentUser.set(null)
    await push('/login')
    path = router.location
  }

  function pageTitle() {
    if (path.startsWith('/browse')) return 'Discover'
    if (path.startsWith('/match')) return 'Match'
    if (path.startsWith('/map')) return 'Map files'
    if (path.startsWith('/approvals')) return 'Approvals'
    if (path.startsWith('/settings')) return 'Settings'
    if (path.startsWith('/users')) return 'Users'
    if (path.startsWith('/api-key')) return 'API Key'
    if (path.startsWith('/password') || path.startsWith('/account')) return 'Account'
    return 'Queue'
  }
</script>

{#if loading}
  <div class="auth-wrap"><p class="muted">Loading Audiobooker…</p></div>
{:else if user && path !== '/login' && path !== '/setup'}
  <div class="shell" class:is-root={user.role === 'root'}>
    <header class="topbar">
      <div class="topbar-main">
        <div class="brand-block">
          <div class="brand">Audiobooker</div>
          <div class="page-title">{pageTitle()}</div>
        </div>
        <button
          class="menu-toggle secondary"
          type="button"
          aria-expanded={menuOpen}
          aria-label="Open menu"
          onclick={() => (menuOpen = !menuOpen)}
        >
          {menuOpen ? 'Close' : 'Menu'}
        </button>
      </div>

      <nav class="nav-desktop" aria-label="Primary">
        {#if isOperator(user)}
          <a href="#/" class:active={path === '/'}>Queue</a>
          <a href="#/browse" class:active={path.startsWith('/browse')}>Discover</a>
        {/if}
        {#if canApprove(user)}
          <a href="#/approvals" class:active={path.startsWith('/approvals')}>Approvals</a>
        {/if}
        {#if isRoot(user)}
          <a href="#/settings" class:active={path === '/settings'}>Settings</a>
          <a href="#/users" class:active={path === '/users'}>Users</a>
          <a href="#/api-key" class:active={path === '/api-key'}>API Key</a>
        {/if}
        <a href="#/account" class:active={path === '/password' || path === '/account'}>Account</a>
        <button class="linkish" type="button" onclick={logout}>Sign out</button>
      </nav>

      {#if menuOpen}
        <nav class="nav-drawer" aria-label="Menu">
          {#if isOperator(user)}
            <a href="#/" class:active={path === '/'}>Queue</a>
            <a href="#/browse" class:active={path.startsWith('/browse')}>Discover</a>
          {/if}
          {#if canApprove(user)}
            <a href="#/approvals" class:active={path.startsWith('/approvals')}>Approvals</a>
          {/if}
          {#if isRoot(user)}
            <a href="#/settings" class:active={path === '/settings'}>Settings</a>
            <a href="#/users" class:active={path === '/users'}>Users</a>
            <a href="#/api-key" class:active={path === '/api-key'}>API Key</a>
          {/if}
          <a href="#/account" class:active={path === '/password' || path.startsWith('/account')}>Account</a>
          <button class="linkish" type="button" onclick={logout}>Sign out</button>
        </nav>
      {/if}
    </header>

    <main class="main">
      <Router routes={authedRoutes} />
    </main>

    <footer class="app-credit">
      <CreatedBy />
    </footer>

    {#if isOperator(user)}
      <nav class="bottom-nav" aria-label="Primary">
        <a
          href="#/"
          aria-label="Queue"
          class:active={path === '/' || path.startsWith('/match') || path.startsWith('/map')}
        >
          <BottomNavIcon name="queue" />
        </a>
        <a href="#/browse" aria-label="Discover" class:active={path.startsWith('/browse')}>
          <BottomNavIcon name="discover" />
        </a>
        {#if canApprove(user)}
          <a href="#/approvals" aria-label="Approvals" class:active={path.startsWith('/approvals')}>
            <BottomNavIcon name="approvals" />
          </a>
        {/if}
        <a
          href="#/account"
          aria-label="Account"
          class:active={path.startsWith('/password') || path.startsWith('/account')}
        >
          <BottomNavIcon name="account" />
        </a>
      </nav>
    {:else if isRoot(user)}
      <nav class="bottom-nav" aria-label="Primary">
        <a href="#/approvals" aria-label="Approvals" class:active={path.startsWith('/approvals')}>
          <BottomNavIcon name="approvals" />
        </a>
        <a href="#/settings" aria-label="Settings" class:active={path === '/settings'}>
          <BottomNavIcon name="settings" />
        </a>
        <a href="#/users" aria-label="Users" class:active={path === '/users'}>
          <BottomNavIcon name="users" />
        </a>
        <a href="#/api-key" aria-label="API Key" class:active={path === '/api-key'}>
          <BottomNavIcon name="api" />
        </a>
      </nav>
    {/if}
  </div>
{:else}
  <div class="public-shell">
    <Router routes={publicRoutes} />
    <footer class="app-credit">
      <CreatedBy />
    </footer>
  </div>
{/if}

{#if toastMsg}
  <div class="toast">{toastMsg}</div>
{/if}
