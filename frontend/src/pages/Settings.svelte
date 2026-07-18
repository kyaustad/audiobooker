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
  let drafts = $state<Record<number, { name: string; path: string }>>({})
  let savingLib = $state<number | null>(null)
  let newName = $state('')
  let newPath = $state('')
  let syncing = $state(false)
  let syncingUsers = $state(false)
  let absDefaultPassword = $state('')

  function needsPath(path: string) {
    const p = path.trim()
    return !p || p.startsWith('__unset__')
  }

  function syncDrafts(libs: Library[]) {
    const next: Record<number, { name: string; path: string }> = {}
    for (const lib of libs) {
      next[lib.id] = {
        name: lib.name,
        path: needsPath(lib.path) ? '' : lib.path,
      }
    }
    drafts = next
  }

  async function refreshLibraries() {
    const data = await api.listLibraries()
    libraries = data.libraries
    syncDrafts(data.libraries)
  }

  onMount(async () => {
    try {
      const data = await api.getSettings()
      settings = {
        rate_limit_requests: 0,
        rate_limit_window_secs: 86400,
        rate_limit_active_torrents: 0,
        ...data.settings,
      }
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
      if (absDefaultPassword) body.abs_user_default_password = absDefaultPassword
      delete body.qbittorrent_password_set
      delete body.vapid_configured
      delete body.vapid_public_key
      delete body.audiobookshelf_token_set
      delete body.abs_user_default_password_set
      delete body.abs_user_last_sync_at
      const data = await api.updateSettings(body)
      settings = data.settings
      password = ''
      absToken = ''
      absDefaultPassword = ''
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

  async function saveLibrary(lib: Library) {
    const draft = drafts[lib.id]
    if (!draft?.name.trim() || !draft.path.trim()) {
      showToast('Name and container path are required')
      return
    }
    savingLib = lib.id
    try {
      await api.updateLibrary(lib.id, draft.name.trim(), draft.path.trim())
      await refreshLibraries()
      showToast(`Saved ${draft.name.trim()}`)
    } catch (err) {
      showToast(err instanceof Error ? err.message : 'Failed to save library')
    } finally {
      savingLib = null
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
      syncDrafts(data.libraries)
      const pending = data.needs_path ?? data.libraries.filter((l) => needsPath(l.path)).length
      if (pending > 0) {
        showToast(`Synced ${data.imported} libraries — set container paths for ${pending}`)
      } else {
        showToast(`Synced ${data.imported} libraries from Audiobookshelf`)
      }
    } catch (err) {
      showToast(err instanceof Error ? err.message : 'ABS sync failed')
    } finally {
      syncing = false
    }
  }

  async function syncAbsUsers() {
    syncingUsers = true
    try {
      // Persist form values first so sync uses the latest URL/token/password.
      const body: Record<string, unknown> = { ...settings }
      if (absToken) body.audiobookshelf_token = absToken
      if (absDefaultPassword) body.abs_user_default_password = absDefaultPassword
      delete body.qbittorrent_password_set
      delete body.vapid_configured
      delete body.vapid_public_key
      delete body.audiobookshelf_token_set
      delete body.abs_user_default_password_set
      delete body.abs_user_last_sync_at
      const saved = await api.updateSettings(body)
      settings = saved.settings
      absToken = ''
      absDefaultPassword = ''

      const data = await api.syncAbsUsers()
      settings = data.settings
      showToast(
        `Users: ${data.created} created, ${data.linked} linked, ${data.updated_libraries} library updates (${data.skipped} skipped)`,
      )
    } catch (err) {
      showToast(err instanceof Error ? err.message : 'ABS user sync failed')
    } finally {
      syncingUsers = false
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
        <input bind:value={settings.library_path} placeholder="optional" />
      </label>
    </div>
    <p class="muted" style="margin:0">
      Fallback is only used when a download has no library assigned. Prefer per-library paths below.
    </p>
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
        Sync imports library names from ABS. Container paths are assigned here — ABS folder paths are only a hint.
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

    <div>
      <h3 style="margin:0.5rem 0 0.25rem">ABS user sync</h3>
      <p class="muted" style="margin:0">
        Pull regular ABS users into Audiobooker. Passwords cannot be read from ABS — new accounts get a
        default password and must change it on first login. OpenID would share login via an identity
        provider, not sync ABS passwords.
      </p>
    </div>
    <label class="check-row">
      <input type="checkbox" bind:checked={settings.abs_user_sync_enabled} />
      Automatically sync ABS users on a schedule
    </label>
    <div class="row">
      <label>User sync interval (ms)
        <input type="number" min="60000" step="60000" bind:value={settings.abs_user_sync_interval_ms} />
      </label>
      <label>Default password for new users {settings.abs_user_default_password_set ? '(saved — leave blank to keep)' : ''}
        <input
          bind:value={absDefaultPassword}
          type="password"
          placeholder={settings.abs_user_default_password_set ? 'Leave blank to keep (default changeme)' : 'changeme'}
          autocomplete="new-password"
        />
      </label>
    </div>
    <label class="check-row">
      <input type="checkbox" bind:checked={settings.abs_user_sync_libraries} />
      Map ABS library access onto Audiobooker libraries (by ABS library id)
    </label>
    {#if settings.abs_user_last_sync_at}
      <p class="muted" style="margin:0">Last user sync: {settings.abs_user_last_sync_at}</p>
    {/if}

    <div>
      <h3 style="margin:0.5rem 0 0.25rem">Rate limits</h3>
      <p class="muted" style="margin:0">
        Global caps for download creates and concurrent torrents. Use 0 for unlimited. Per-user
        overrides are on the Users page.
      </p>
    </div>
    <div class="row">
      <label>Max requests per window
        <input type="number" min="0" bind:value={settings.rate_limit_requests} />
      </label>
      <label>Window (seconds)
        <input type="number" min="60" bind:value={settings.rate_limit_window_secs} />
      </label>
      <label>Max active torrents
        <input type="number" min="0" bind:value={settings.rate_limit_active_torrents} />
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
      <button class="secondary" type="button" disabled={syncingUsers} onclick={syncAbsUsers}>
        {syncingUsers ? 'Syncing users…' : 'Sync users from ABS'}
      </button>
    </div>
  </form>

  <div class="card stack">
    <div>
      <h2>Libraries</h2>
      <p class="muted">
        Add volume mounts for your library folders on the container, then put those
        <strong>container</strong> paths here. ABS sync brings in library names only — it never overwrites paths you set.
      </p>
    </div>
    <form class="row" onsubmit={addLibrary}>
      <label>Name
        <input bind:value={newName} required placeholder="Audiobooks" />
      </label>
      <label>Container path
        <input bind:value={newPath} required placeholder="/media/audiobooks" />
      </label>
      <button type="submit">Add</button>
    </form>
    <div class="stack lib-list">
      {#each libraries as lib}
        {@const draft = drafts[lib.id]}
        <div class="lib-row" class:needs-path={needsPath(lib.path)}>
          <div class="row lib-fields">
            <label>Name
              <input
                value={draft?.name ?? lib.name}
                oninput={(e) => {
                  drafts[lib.id] = {
                    name: (e.currentTarget as HTMLInputElement).value,
                    path: drafts[lib.id]?.path ?? '',
                  }
                }}
              />
            </label>
            <label>Container path
              <input
                value={draft?.path ?? ''}
                placeholder={lib.abs_path ? `ABS reports ${lib.abs_path}` : '/path/in/container'}
                oninput={(e) => {
                  drafts[lib.id] = {
                    name: drafts[lib.id]?.name ?? lib.name,
                    path: (e.currentTarget as HTMLInputElement).value,
                  }
                }}
              />
            </label>
          </div>
          {#if lib.abs_path}
            <p class="muted abs-hint">ABS folder: {lib.abs_path}</p>
          {/if}
          {#if needsPath(lib.path)}
            <p class="warn">Set the container mount path before imports can use this library.</p>
          {/if}
          <div class="row lib-actions">
            <button type="button" disabled={savingLib === lib.id} onclick={() => saveLibrary(lib)}>
              {savingLib === lib.id ? 'Saving…' : 'Save'}
            </button>
            <button class="danger" type="button" onclick={() => removeLibrary(lib.id)}>Remove</button>
          </div>
        </div>
      {:else}
        <p class="muted">No libraries yet — sync from ABS or add one manually.</p>
      {/each}
    </div>
  </div>
{/if}

<style>
  .lib-list {
    gap: 0.85rem;
  }
  .lib-row {
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 0.75rem 0.85rem;
    display: flex;
    flex-direction: column;
    gap: 0.45rem;
  }
  .lib-row.needs-path {
    border-color: color-mix(in oklab, #c45c26 55%, var(--border));
  }
  .lib-fields {
    align-items: end;
  }
  .lib-actions {
    justify-content: flex-end;
  }
  .abs-hint {
    margin: 0;
    font-size: 0.85rem;
  }
  .warn {
    margin: 0;
    font-size: 0.88rem;
    color: #c45c26;
  }
  .check-row {
    display: flex;
    align-items: center;
    gap: 0.55rem;
    color: var(--text);
    font-size: 0.92rem;
  }
</style>
