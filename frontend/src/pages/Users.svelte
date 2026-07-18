<script lang="ts">
  import { onMount } from 'svelte'
  import { api, type Library, type User } from '../lib/api'
  import { showToast } from '../lib/toast'

  let users = $state<User[]>([])
  let libraries = $state<Library[]>([])
  let username = $state('')
  let password = $state('')
  let createLibraryIds = $state<number[]>([])
  let loading = $state(true)
  let editingId = $state<number | null>(null)
  let editPassword = $state('')
  let editLibraryIds = $state<number[]>([])
  let editRateRequests = $state('')
  let editRateWindow = $state('')
  let editRateActive = $state('')

  async function refresh() {
    const [u, libs] = await Promise.all([api.listUsers(), api.listLibraries()])
    users = u.users
    libraries = libs.libraries
    if (!createLibraryIds.length) {
      createLibraryIds = libraries.map((l) => l.id)
    }
  }

  onMount(() => {
    refresh()
      .catch((e) => showToast(e.message))
      .finally(() => (loading = false))
  })

  function toggleCreateLib(id: number) {
    createLibraryIds = createLibraryIds.includes(id)
      ? createLibraryIds.filter((x) => x !== id)
      : [...createLibraryIds, id]
  }

  function toggleEditLib(id: number) {
    editLibraryIds = editLibraryIds.includes(id)
      ? editLibraryIds.filter((x) => x !== id)
      : [...editLibraryIds, id]
  }

  async function create(e: Event) {
    e.preventDefault()
    try {
      await api.createUser(username, password, createLibraryIds)
      showToast('User created — they must change password on first login')
      username = ''
      password = ''
      await refresh()
    } catch (err) {
      showToast(err instanceof Error ? err.message : 'Failed')
    }
  }

  function startEdit(u: User) {
    editingId = u.id
    editPassword = ''
    editLibraryIds = [...(u.library_ids || u.libraries?.map((l) => l.id) || [])]
    editRateRequests = u.rate_limit_requests == null ? '' : String(u.rate_limit_requests)
    editRateWindow = u.rate_limit_window_secs == null ? '' : String(u.rate_limit_window_secs)
    editRateActive = u.rate_limit_active_torrents == null ? '' : String(u.rate_limit_active_torrents)
  }

  function parseOverride(raw: string): number | null {
    const t = raw.trim()
    if (!t) return null
    const n = Number(t)
    return Number.isFinite(n) ? n : null
  }

  async function saveEdit(id: number) {
    try {
      await api.updateUser(id, {
        library_ids: editLibraryIds,
        rate_limit_requests: parseOverride(editRateRequests),
        rate_limit_window_secs: parseOverride(editRateWindow),
        rate_limit_active_torrents: parseOverride(editRateActive),
        ...(editPassword ? { password: editPassword, must_change_password: true } : {}),
      })
      showToast('User updated')
      editingId = null
      await refresh()
    } catch (err) {
      showToast(err instanceof Error ? err.message : 'Failed')
    }
  }

  async function remove(id: number) {
    try {
      await api.deleteUser(id)
      showToast('User removed')
      await refresh()
    } catch (err) {
      showToast(err instanceof Error ? err.message : 'Failed')
    }
  }
</script>

<div class="card stack">
  <div>
    <h2>Users</h2>
    <p class="muted">
      Create accounts and choose which Audiobookshelf libraries each user may import into.
    </p>
  </div>
  <form class="stack" onsubmit={create}>
    <div class="row">
      <label>Username
        <input bind:value={username} required minlength="3" />
      </label>
      <label>Temp password
        <input bind:value={password} type="password" required minlength="8" />
      </label>
    </div>
    {#if libraries.length}
      <div class="lib-picker">
        <div class="lib-picker-label">Libraries</div>
        <div class="lib-list">
          {#each libraries as lib}
            <label class="lib-option">
              <input
                type="checkbox"
                checked={createLibraryIds.includes(lib.id)}
                onchange={() => toggleCreateLib(lib.id)}
              />
              <span class="lib-copy">
                <span class="lib-name">{lib.name}</span>
                <span class="lib-path">{lib.path}</span>
              </span>
            </label>
          {/each}
        </div>
      </div>
    {:else}
      <p class="muted">Add libraries under Settings first.</p>
    {/if}
    <button type="submit">Create user</button>
  </form>
</div>

<div class="card">
  {#if loading}
    <p class="muted">Loading…</p>
  {:else}
    <div class="stack">
      {#each users as u}
        <div class="user-row">
          <div class="user-meta">
            <div class="user-title">
              <strong>{u.username}</strong>
              <span class="badge">{u.role}</span>
              {#if u.must_change_password}
                <span class="badge awaiting_match">must change password</span>
              {/if}
              {#if u.abs_user_id}
                <span class="badge completed">ABS</span>
              {/if}
            </div>
            {#if u.role !== 'root'}
              <div class="muted user-libs">
                {(u.libraries || []).map((l) => l.name).join(', ') || 'No libraries'}
              </div>
              {#if u.rate_limit_requests != null || u.rate_limit_active_torrents != null}
                <div class="muted user-libs">
                  Rate limits:
                  {#if u.rate_limit_requests != null}{u.rate_limit_requests} req{/if}
                  {#if u.rate_limit_window_secs != null} / {u.rate_limit_window_secs}s{/if}
                  {#if u.rate_limit_active_torrents != null}
                    · {u.rate_limit_active_torrents} active
                  {/if}
                </div>
              {/if}
            {/if}
          </div>
          {#if u.role !== 'root'}
            <div class="user-actions">
              <button class="secondary" type="button" onclick={() => startEdit(u)}>Edit</button>
              <button class="danger" type="button" onclick={() => remove(u.id)}>Delete</button>
            </div>
          {/if}
        </div>

        {#if editingId === u.id}
          <div class="edit-panel stack">
            <label>Reset password (optional)
              <input bind:value={editPassword} type="password" minlength="8" placeholder="Leave blank to keep" />
            </label>
            <div class="row rate-row">
              <label>Req limit (blank = global)
                <input bind:value={editRateRequests} type="number" min="0" placeholder="Global" />
              </label>
              <label>Window secs
                <input bind:value={editRateWindow} type="number" min="60" placeholder="Global" />
              </label>
              <label>Active torrents
                <input bind:value={editRateActive} type="number" min="0" placeholder="Global" />
              </label>
            </div>
            <div class="lib-picker">
              <div class="lib-picker-label">Libraries</div>
              <div class="lib-list">
                {#each libraries as lib}
                  <label class="lib-option">
                    <input
                      type="checkbox"
                      checked={editLibraryIds.includes(lib.id)}
                      onchange={() => toggleEditLib(lib.id)}
                    />
                    <span class="lib-copy">
                      <span class="lib-name">{lib.name}</span>
                      <span class="lib-path">{lib.path}</span>
                    </span>
                  </label>
                {/each}
              </div>
            </div>
            <div class="user-actions">
              <button type="button" onclick={() => saveEdit(u.id)}>Save</button>
              <button class="secondary" type="button" onclick={() => (editingId = null)}>Cancel</button>
            </div>
          </div>
        {/if}
      {/each}
    </div>
  {/if}
</div>

<style>
  .lib-picker {
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--bg);
    overflow: hidden;
  }
  .lib-picker-label {
    padding: 0.5rem 0.85rem;
    font-size: 0.85rem;
    color: var(--muted);
    border-bottom: 1px solid var(--border);
  }
  .lib-list {
    display: flex;
    flex-direction: column;
  }
  .lib-option {
    display: flex !important;
    flex: none !important;
    flex-direction: row !important;
    align-items: flex-start;
    gap: 0.7rem;
    margin: 0;
    padding: 0.7rem 0.85rem;
    color: var(--text);
    border-bottom: 1px solid var(--border);
    cursor: pointer;
  }
  .lib-option:last-child {
    border-bottom: none;
  }
  .lib-option:hover {
    background: var(--bg-hover);
  }
  .lib-option input[type='checkbox'] {
    width: 1.05rem !important;
    height: 1.05rem;
    margin: 0.15rem 0 0;
    flex: 0 0 auto;
    accent-color: var(--accent);
  }
  .lib-copy {
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
    min-width: 0;
  }
  .lib-name {
    font-weight: 600;
    color: var(--text);
  }
  .lib-path {
    font-size: 0.82rem;
    color: var(--muted);
    font-family: var(--mono);
    word-break: break-all;
  }
  .user-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 0.75rem;
    border-bottom: 1px solid var(--border);
    padding-bottom: 0.7rem;
  }
  .user-title {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.35rem;
  }
  .user-libs {
    margin-top: 0.25rem;
  }
  .user-actions {
    display: flex;
    gap: 0.5rem;
    flex-shrink: 0;
  }
  .edit-panel {
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 0.85rem;
    margin-bottom: 0.75rem;
  }
  .rate-row {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(140px, 1fr));
    gap: 0.65rem;
  }
</style>
