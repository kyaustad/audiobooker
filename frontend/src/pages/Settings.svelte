<script lang="ts">
  import { onMount } from 'svelte'
  import { api, type Library } from '../lib/api'
  import { showToast } from '../lib/toast'

  let settings = $state<any>({})
  let password = $state('')
  let absToken = $state('')
  let loading = $state(true)
  let saving = $state(false)
  let testing = $state(false)
  let libraries = $state<Library[]>([])
  let newName = $state('')
  let newPath = $state('')
  let syncing = $state(false)

  async function refreshLibraries() {
    const data = await api.listLibraries()
    libraries = data.libraries
  }

  onMount(async () => {
    try {
      const data = await api.getSettings()
      settings = data.settings
      await refreshLibraries()
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
      if (absToken) body.audiobookshelf_token = absToken
      delete body.qbittorrent_password_set
      delete body.vapid_configured
      delete body.vapid_public_key
      delete body.audiobookshelf_token_set
      const data = await api.updateSettings(body)
      settings = data.settings
      password = ''
      absToken = ''
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

  async function addLibrary(e: Event) {
    e.preventDefault()
    try {
      await api.createLibrary(newName, newPath)
      newName = ''
      newPath = ''
      await refreshLibraries()
      showToast('Library added')
    } catch (err) {
      showToast(err instanceof Error ? err.message : 'Failed')
    }
  }

  async function removeLibrary(id: number) {
    try {
      await api.deleteLibrary(id)
      await refreshLibraries()
      showToast('Library removed')
    } catch (err) {
      showToast(err instanceof Error ? err.message : 'Failed')
    }
  }

  async function syncAbs() {
    syncing = true
    try {
      const data = await api.syncAbsLibraries({
        audiobookshelf_url: settings.audiobookshelf_url || undefined,
        audiobookshelf_token: absToken || undefined,
      })
      libraries = data.libraries
      showToast(`Synced ${data.imported} libraries from Audiobookshelf`)
    } catch (err) {
      showToast(err instanceof Error ? err.message : 'ABS sync failed')
    } finally {
      syncing = false
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
      <label>Fallback library path
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

    <div>
      <h3 style="margin:0.5rem 0 0.25rem">Audiobookshelf</h3>
      <p class="muted" style="margin:0">
        Optional API connection to import library folders. Paths must match this container’s mounts.
      </p>
    </div>
    <div class="row">
      <label>Audiobookshelf URL
        <input bind:value={settings.audiobookshelf_url} placeholder="http://audiobookshelf:80" />
      </label>
      <label>API token {settings.audiobookshelf_token_set ? '(saved — leave blank to keep)' : ''}
        <input bind:value={absToken} type="password" placeholder={settings.audiobookshelf_token_set ? 'Leave blank to keep' : 'ABS API token'} autocomplete="new-password" />
      </label>
    </div>

    <div class="row">
      <button type="submit" disabled={saving}>{saving ? 'Saving…' : 'Save settings'}</button>
      <button class="secondary" type="button" disabled={testing} onclick={test}>
        {testing ? 'Testing…' : 'Test qBittorrent'}
      </button>
      <button class="secondary" type="button" disabled={syncing} onclick={syncAbs}>
        {syncing ? 'Syncing…' : 'Sync libraries from ABS'}
      </button>
    </div>
  </form>

  <div class="card stack">
    <div>
      <h2>Libraries</h2>
      <p class="muted">
        Each entry is an Audiobookshelf library folder mounted into this container. Assign them to users under Users.
      </p>
    </div>
    <form class="row" onsubmit={addLibrary}>
      <label>Name
        <input bind:value={newName} required placeholder="Fiction" />
      </label>
      <label>Container path
        <input bind:value={newPath} required placeholder="/audiobooks/fiction" />
      </label>
      <button type="submit">Add</button>
    </form>
    <div class="stack">
      {#each libraries as lib}
        <div class="row" style="justify-content:space-between;align-items:center;border-bottom:1px solid var(--border);padding-bottom:0.55rem">
          <div>
            <strong>{lib.name}</strong>
            <div class="muted">{lib.path}</div>
          </div>
          <button class="danger" type="button" onclick={() => removeLibrary(lib.id)}>Remove</button>
        </div>
      {:else}
        <p class="muted">No libraries yet.</p>
      {/each}
    </div>
  </div>
{/if}
