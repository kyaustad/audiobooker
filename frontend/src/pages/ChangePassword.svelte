<script lang="ts">
  import { push } from 'svelte-spa-router'
  import { api } from '../lib/api'
  import { currentUser } from '../lib/session'
  import { showToast } from '../lib/toast'

  let current_password = $state('')
  let new_password = $state('')
  let loading = $state(false)

  async function submit(e: Event) {
    e.preventDefault()
    loading = true
    try {
      await api.changePassword(current_password, new_password)
      currentUser.update((u) => (u ? { ...u, must_change_password: false } : u))
      showToast('Password updated')
      const u = await api.me()
      currentUser.set(u.user)
      push(u.user.role === 'root' ? '/settings' : '/')
    } catch (err) {
      showToast(err instanceof Error ? err.message : 'Failed to update password')
    } finally {
      loading = false
    }
  }
</script>

<div class="card stack" style="max-width:480px">
  <div>
    <h2>Change password</h2>
    <p class="muted">Use a new password for your account.</p>
  </div>
  <form class="stack" onsubmit={submit}>
    <label>Current password
      <input bind:value={current_password} type="password" required />
    </label>
    <label>New password
      <input bind:value={new_password} type="password" required minlength="8" />
    </label>
    <button type="submit" disabled={loading}>{loading ? 'Saving…' : 'Update password'}</button>
  </form>
</div>
