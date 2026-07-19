<script lang="ts">
  import { onMount } from 'svelte'
  import { api, type Library, type User } from '../lib/api'
  import { showToast } from '../lib/toast'

  let users = $state<User[]>([])
  let libraries = $state<Library[]>([])
  let username = $state('')
  let password = $state('')
  let createRole = $state('user')
  let createCanRemove = $state(true)
  let createCanRemoveFiles = $state(false)
  let createLibraryIds = $state<number[]>([])
  let loading = $state(true)
  let editingId = $state<number | null>(null)
  let editPassword = $state('')
  let editRole = $state('user')
  let editCanRemove = $state(true)
  let editCanRemoveFiles = $state(false)
  let editLibraryIds = $state<number[]>([])
  let editRateRequests = $state('')
  let editRateWindow = $state('')
  let editRateActive = $state('')

  const ROLE_HELP: Record<string, string> = {
    requester: 'Browse & request — must match Audible, then await approval',
    user: 'Full download access — match and start immediately',
    approver: 'Like user, plus approve requester downloads',
  }

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
      await api.createUser(username, password, createLibraryIds, {
        role: createRole,
        can_remove: createCanRemove,
        can_remove_files: createCanRemove && createCanRemoveFiles,
      })
      showToast('User created — they must change password on first login')
      username = ''
      password = ''
      createRole = 'user'
      createCanRemove = true
      createCanRemoveFiles = false
      await refresh()
    } catch (err) {
      showToast(err instanceof Error ? err.message : 'Failed')
    }
  }

  function startEdit(u: User) {
    editingId = u.id
    editPassword = ''
    editRole = u.role
    editCanRemove = u.can_remove !== false
    editCanRemoveFiles = Boolean(u.can_remove_files)
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
        role: editRole,
        can_remove: editCanRemove,
        can_remove_files: editCanRemove && editCanRemoveFiles,
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
    if (!window.confirm('Delete this user and their sessions? Downloads stay owned by the user id until cleaned up.')) {
      return
    }
    try {
      await api.deleteUser(id)
      showToast('User removed')
      await refresh()
    } catch (err) {
      showToast(err instanceof Error ? err.message : 'Failed')
    }
  }

  function removeLabel(u: User) {
    if (u.can_remove === false) return 'Remove: off'
    if (u.can_remove_files) return 'Remove: torrent + files'
    return 'Remove: torrent only'
  }
</script>

<div class="card stack">
  <div>
    <h2>Users</h2>
    <p class="muted">
      Create accounts, assign libraries, and set roles. Requesters match Audible before an approver starts the download.
      Torrents are tagged in qBittorrent with each user’s username.
    </p>
  </div>

  <form class="stack create-form" onsubmit={create}>
    <div class="field-grid">
      <label>Username
        <input bind:value={username} required minlength="3" autocomplete="off" />
      </label>
      <label>Temp password
        <input bind:value={password} type="password" required minlength="8" autocomplete="new-password" />
      </label>
      <label>Role
        <select bind:value={createRole}>
          <option value="requester">requester</option>
          <option value="user">user</option>
          <option value="approver">approver</option>
        </select>
      </label>
    </div>
    <p class="muted role-help">{ROLE_HELP[createRole]}</p>

    <div class="perm-panel">
      <div class="perm-title">Remove permissions</div>
      <div class="perm-options">
        <label class="check">
          <input type="checkbox" bind:checked={createCanRemove} />
          <span>
            <strong>Remove torrents</strong>
            <span class="muted">Drop from queue / qBittorrent</span>
          </span>
        </label>
        <label class="check" class:dim={!createCanRemove}>
          <input
            type="checkbox"
            bind:checked={createCanRemoveFiles}
            disabled={!createCanRemove}
          />
          <span>
            <strong>Delete downloaded files</strong>
            <span class="muted">Also wipe torrent data on disk</span>
          </span>
        </label>
      </div>
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
  {:else if !users.length}
    <div class="empty-state">
      <strong>No users yet</strong>
      <p class="muted">Create a requester, user, or approver above.</p>
    </div>
  {:else}
    <div class="user-list">
      {#each users as u}
        <div class="user-block">
          <div class="user-row">
            <div class="user-meta">
              <div class="user-title">
                <strong>{u.username}</strong>
                <span class={`badge role-${u.role}`}>{u.role}</span>
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
                <div class="muted user-libs">{removeLabel(u)}</div>
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
              <div class="field-grid">
                <label>Role
                  <select bind:value={editRole}>
                    <option value="requester">requester</option>
                    <option value="user">user</option>
                    <option value="approver">approver</option>
                  </select>
                </label>
                <label>Reset password
                  <input bind:value={editPassword} type="password" minlength="8" placeholder="Leave blank to keep" />
                </label>
              </div>
              <p class="muted role-help">{ROLE_HELP[editRole]}</p>

              <div class="perm-panel">
                <div class="perm-title">Remove permissions</div>
                <div class="perm-options">
                  <label class="check">
                    <input type="checkbox" bind:checked={editCanRemove} />
                    <span>
                      <strong>Remove torrents</strong>
                      <span class="muted">Drop from queue / qBittorrent</span>
                    </span>
                  </label>
                  <label class="check" class:dim={!editCanRemove}>
                    <input
                      type="checkbox"
                      bind:checked={editCanRemoveFiles}
                      disabled={!editCanRemove}
                    />
                    <span>
                      <strong>Delete downloaded files</strong>
                      <span class="muted">Also wipe torrent data on disk</span>
                    </span>
                  </label>
                </div>
              </div>

              <div class="field-grid rate-grid">
                <label>Req limit <span class="muted">(blank = global)</span>
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
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .create-form {
    padding-top: 0.25rem;
  }
  .field-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(160px, 1fr));
    gap: 0.75rem;
  }
  .rate-grid {
    grid-template-columns: repeat(auto-fit, minmax(140px, 1fr));
  }
  .role-help {
    margin: -0.25rem 0 0;
    font-size: 0.88rem;
  }
  .perm-panel {
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg);
    overflow: hidden;
  }
  .perm-title {
    padding: 0.5rem 0.85rem;
    font-size: 0.85rem;
    color: var(--muted);
    border-bottom: 1px solid var(--border);
  }
  .perm-options {
    display: grid;
    gap: 0;
  }
  .check {
    display: flex !important;
    flex-direction: row !important;
    align-items: flex-start;
    gap: 0.7rem;
    margin: 0;
    padding: 0.75rem 0.85rem;
    color: var(--text);
    border-bottom: 1px solid var(--border);
    cursor: pointer;
  }
  .check:last-child {
    border-bottom: none;
  }
  .check.dim {
    opacity: 0.5;
  }
  .check input {
    margin-top: 0.2rem;
  }
  .check span {
    display: grid;
    gap: 0.15rem;
  }
  .check strong {
    font-size: 0.92rem;
  }
  .empty-state {
    text-align: center;
    padding: 1.75rem 1rem;
  }
  .user-list {
    display: grid;
    gap: 0;
  }
  .user-block {
    border-bottom: 1px solid var(--border);
    padding: 0.9rem 0;
  }
  .user-block:last-child {
    border-bottom: none;
    padding-bottom: 0;
  }
  .user-block:first-child {
    padding-top: 0;
  }
  .user-row {
    display: flex;
    justify-content: space-between;
    gap: 0.85rem;
    align-items: flex-start;
    flex-wrap: wrap;
  }
  .user-title {
    display: flex;
    flex-wrap: wrap;
    gap: 0.35rem;
    align-items: center;
  }
  .user-libs {
    margin-top: 0.3rem;
    font-size: 0.88rem;
  }
  .user-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 0.4rem;
  }
  .edit-panel {
    margin-top: 0.85rem;
    padding: 0.9rem;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg);
  }
  .lib-picker {
    border: 1px solid var(--border);
    border-radius: 8px;
    background: color-mix(in oklab, var(--bg-elevated) 80%, black);
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
  .lib-copy {
    display: grid;
    gap: 0.15rem;
    min-width: 0;
  }
  .lib-name {
    font-weight: 600;
    color: var(--text);
  }
  .lib-path {
    font-size: 0.8rem;
    color: var(--muted);
    word-break: break-all;
  }
  @media (max-width: 640px) {
    .user-actions {
      width: 100%;
    }
    .user-actions button {
      flex: 1 1 auto;
    }
  }
</style>
