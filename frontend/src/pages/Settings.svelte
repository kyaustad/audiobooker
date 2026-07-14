<script lang="ts">
  import { onMount } from 'svelte'
  import { api } from '../lib/api'
  import { showToast } from '../lib/toast'

  let settings = $state<any>({})
  let password = $state('')
  let loading = $state(true)
  let saving = $state(false)
  let testing = $state(false)

  onMount(async () => {
    try {
      const data = await api.getSettings()
      settings = data.settings
    } catch (err) {
      showToast(err instanceof Error ? err.message : 'Failed to load settings')
    } finally {
      loading = false
    }
  })

  async function save(e: Event) {
    e.preventDefault()
    saving = true
    try {
      const body: Record<string, unknown> = { ...settings }
      if (password) body.qbittorrent_password = password
      delete body.qbittorrent_password_set
      delete body.vapid_configured
      delete body.vapid_public_key
      const data = await api.updateSettings(body)
      settings = data.settings
      password = ''
      showToast('Settings saved')
    } catch (err) {
      showToast(err instanceof Error ? err.message : 'Save failed')
    } finally {
      saving = false
    }
  }

  async function test() {
    testing = true
    try {
      // Send current form values so Test works before Save
      await api.testQbit({
        qbittorrent_url: settings.qbittorrent_url || '',
        qbittorrent_username: settings.qbittorrent_username || '',
        qbittorrent_password: password || undefined,
      })
      showToast('qBittorrent connection OK')
    } catch (err) {
      showToast(err instanceof Error ? err.message : 'Connection failed')
    } finally {
      testing = false
    }
  }
</script>

{#if loading}
  <p class="muted">Loading settings…</p>
{:else}
  <form class="card stack" onsubmit={save}>
    <div>
      <h2>Download client</h2>
      <p class="muted">
        Configure the qBittorrent WebUI (URL + username/password). Test uses the values in this form
        even if you have not saved yet.
      </p>
    </div>
    <label>qBittorrent URL
      <input bind:value={settings.qbittorrent_url} placeholder="http://10.0.0.2:8080" />
    </label>
    <div class="row">
      <label>WebUI username
        <input bind:value={settings.qbittorrent_username} autocomplete="off" />
      </label>
      <label>WebUI password {settings.qbittorrent_password_set ? '(saved — leave blank to keep)' : ''}
        <input bind:value={password} type="password" placeholder={settings.qbittorrent_password_set ? 'Leave blank to keep' : 'Required for WebUI auth'} autocomplete="new-password" />
      </label>
    </div>
    <div class="row">
      <label>Download path (in container)
        <input bind:value={settings.download_path} />
      </label>
      <label>Library path (in container)
        <input bind:value={settings.library_path} />
      </label>
    </div>
    <label>Path template
      <input bind:value={settings.path_template} />
    </label>
    <div class="row">
      <label>Audible region
        <input bind:value={settings.audible_region} placeholder="us" />
      </label>
      <label>Metadata provider URL
        <input bind:value={settings.metadata_provider_url} />
      </label>
    </div>
    <label>Sync interval (ms)
      <input type="number" bind:value={settings.sync_interval_ms} />
    </label>
    <div class="row">
      <button type="submit" disabled={saving}>{saving ? 'Saving…' : 'Save settings'}</button>
      <button class="secondary" type="button" disabled={testing} onclick={test}>
        {testing ? 'Testing…' : 'Test qBittorrent'}
      </button>
    </div>
  </form>
{/if}
