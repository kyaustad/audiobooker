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
  }

  async function saveEdit(id: number) {
    try {
      await api.updateUser(id, {
        library_ids: editLibraryIds,
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
      <fieldset class="libs">
        <legend class="muted">Libraries</legend>
        {#each libraries as lib}
          <label class="check">
            <input
              type="checkbox"
              checked={createLibraryIds.includes(lib.id)}
              onchange={() => toggleCreateLib(lib.id)}
            />
            {lib.name} <span class="muted">({lib.path})</span>
          </label>
        {/each}
      </fieldset>
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
          <div>
            <strong>{u.username}</strong>
            <span class="badge" style="margin-left:0.4rem">{u.role}</span>
            {#if u.must_change_password}
              <span class="badge awaiting_match" style="margin-left:0.25rem">must change password</span>
            {/if}
            {#if u.role !== 'root'}
              <div class="muted" style="margin-top:0.25rem">
                {(u.libraries || []).map((l) => l.name).join(', ') || 'No libraries'}
              </div>
            {/if}
          </div>
          {#if u.role !== 'root'}
            <div class="row">
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
            <fieldset class="libs">
              <legend class="muted">Libraries</legend>
              {#each libraries as lib}
                <label class="check">
                  <input
                    type="checkbox"
                    checked={editLibraryIds.includes(lib.id)}
                    onchange={() => toggleEditLib(lib.id)}
                  />
                  {lib.name} <span class="muted">({lib.path})</span>
                </label>
              {/each}
            </fieldset>
            <div class="row">
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
  .libs {
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 0.65rem 0.8rem;
    margin: 0;
  }
  .check {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    color: var(--text);
    margin: 0.25rem 0;
  }
  .user-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 0.75rem;
    border-bottom: 1px solid var(--border);
    padding-bottom: 0.7rem;
  }
  .edit-panel {
    background: var(--bg);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 0.85rem;
    margin-bottom: 0.75rem;
  }
</style>
