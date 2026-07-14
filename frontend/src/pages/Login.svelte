<script lang="ts">
  import { push } from 'svelte-spa-router'
  import { api } from '../lib/api'
  import { currentUser } from '../lib/session'
  import { showToast } from '../lib/toast'

  let username = $state('')
  let password = $state('')
  let loading = $state(false)

  async function submit(e: Event) {
    e.preventDefault()
    loading = true
    try {
      const { user } = await api.login(username, password)
      currentUser.set(user)
      showToast('Welcome back')
      if (user.must_change_password) push('/password')
      else if (user.role === 'root') push('/settings')
      else push('/')
    } catch (err) {
      showToast(err instanceof Error ? err.message : 'Login failed')
    } finally {
      loading = false
    }
  }
</script>

<div class="auth-wrap">
  <form class="card auth-card stack" onsubmit={submit}>
    <div>
      <h2>Sign in</h2>
      <p class="muted">Access your audiobook download queue.</p>
    </div>
    <label>Username
      <input bind:value={username} autocomplete="username" required />
    </label>
    <label>Password
      <input bind:value={password} type="password" autocomplete="current-password" required />
    </label>
    <button type="submit" disabled={loading}>{loading ? 'Signing in…' : 'Sign in'}</button>
  </form>
</div>
