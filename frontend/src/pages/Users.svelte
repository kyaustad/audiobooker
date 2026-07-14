<script lang="ts">
  import { onMount } from 'svelte'
  import { api, type User } from '../lib/api'
  import { showToast } from '../lib/toast'

  let users = $state<User[]>([])
  let username = $state('')
  let password = $state('')
  let loading = $state(true)

  async function refresh() {
    const data = await api.listUsers()
    users = data.users
  }

  onMount(() => {
    refresh()
      .catch((e) => showToast(e.message))
      .finally(() => (loading = false))
  })

  async function create(e: Event) {
    e.preventDefault()
    try {
      await api.createUser(username, password)
      showToast('User created — they must change password on first login')
      username = ''
      password = ''
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
    <p class="muted">Root creates accounts. New users get a temporary password and must change it.</p>
  </div>
  <form class="row" onsubmit={create}>
    <label>Username
      <input bind:value={username} required minlength="3" />
    </label>
    <label>Temp password
      <input bind:value={password} type="password" required minlength="8" />
    </label>
    <button type="submit">Create user</button>
  </form>
</div>

<div class="card">
  {#if loading}
    <p class="muted">Loading…</p>
  {:else}
    <div class="stack">
      {#each users as u}
        <div class="row" style="justify-content:space-between;align-items:center;border-bottom:1px solid var(--border);padding-bottom:0.6rem">
          <div>
            <strong>{u.username}</strong>
            <span class="badge" style="margin-left:0.4rem">{u.role}</span>
            {#if u.must_change_password}
              <span class="badge awaiting_match" style="margin-left:0.25rem">must change password</span>
            {/if}
          </div>
          {#if u.role !== 'root'}
            <button class="danger" type="button" onclick={() => remove(u.id)}>Delete</button>
          {/if}
        </div>
      {/each}
    </div>
  {/if}
</div>
