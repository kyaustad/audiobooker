<script lang="ts">
  import { push } from 'svelte-spa-router'
  import { api } from '../lib/api'
  import { showToast } from '../lib/toast'

  let username = $state('')
  let password = $state('')
  let qbUrl = $state('')
  let qbUser = $state('admin')
  let qbPass = $state('')
  let loading = $state(false)
  let testing = $state(false)

  async function testQbit() {
    testing = true
    try {
      await api.testQbitSetup({
        qbittorrent_url: qbUrl,
        qbittorrent_username: qbUser,
        qbittorrent_password: qbPass,
      })
      showToast('qBittorrent connection OK')
    } catch (err) {
      showToast(err instanceof Error ? err.message : 'Connection failed')
    } finally {
      testing = false
    }
  }

  async function submit(e: Event) {
    e.preventDefault()
    loading = true
    try {
      await api.setup(username, password, {
        qbittorrent_url: qbUrl || undefined,
        qbittorrent_username: qbUser || undefined,
        qbittorrent_password: qbPass || undefined,
      })
      showToast('Root account created — sign in')
      push('/login')
    } catch (err) {
      showToast(err instanceof Error ? err.message : 'Setup failed')
    } finally {
      loading = false
    }
  }
</script>

<div class="auth-wrap">
  <form class="card auth-card stack" onsubmit={submit} style="width:min(520px,100%)">
    <div>
      <h2>Welcome to Audiobooker</h2>
      <p class="muted">Create the root administrator and optionally configure qBittorrent WebUI auth.</p>
    </div>

    <h3 style="margin:0">Root account</h3>
    <label>Username
      <input bind:value={username} autocomplete="username" required minlength="3" />
    </label>
    <label>Password
      <input bind:value={password} type="password" autocomplete="new-password" required minlength="8" />
    </label>

    <h3 style="margin:0.5rem 0 0">qBittorrent (optional)</h3>
    <p class="muted">WebUI requires username/password. You can finish this later in Settings.</p>
    <label>WebUI URL
      <input bind:value={qbUrl} placeholder="http://10.0.0.2:8080" autocomplete="off" />
    </label>
    <div class="row">
      <label>WebUI username
        <input bind:value={qbUser} autocomplete="off" />
      </label>
      <label>WebUI password
        <input bind:value={qbPass} type="password" autocomplete="new-password" />
      </label>
    </div>
    <div class="row">
      <button type="button" class="secondary" disabled={testing || !qbUrl} onclick={testQbit}>
        {testing ? 'Testing…' : 'Test qBittorrent'}
      </button>
      <button type="submit" disabled={loading}>{loading ? 'Creating…' : 'Create root user'}</button>
    </div>
  </form>
</div>
