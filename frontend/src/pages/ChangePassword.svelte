<script lang="ts">
  import { onMount } from 'svelte'
  import { push } from 'svelte-spa-router'
  import { api, type NotificationPrefs } from '../lib/api'
  import { currentUser } from '../lib/session'
  import { showToast } from '../lib/toast'
  import {
    disableNotifications,
    enableNotifications,
    getPushStatus,
    saveNotificationPrefs,
    sendTestNotification,
  } from '../lib/push'

  let current_password = $state('')
  let new_password = $state('')
  let loading = $state(false)

  let pushBusy = $state(false)
  let prefsBusy = $state(false)
  let pushSubscribed = $state(false)
  let pushSupported = $state(true)
  let needsHttps = $state(false)
  let needsInstall = $state(false)
  let isIos = $state(false)
  let prefs = $state<NotificationPrefs>({
    notify_imported: true,
    notify_download_finished: false,
    notify_pack_ready: true,
    notify_failures: true,
  })

  async function refreshPush() {
    const status = await getPushStatus()
    pushSupported = status.supported
    pushSubscribed = status.subscribed && status.permission === 'granted'
    needsHttps = status.needsHttps
    needsInstall = status.needsInstall
    isIos = status.isIos
    prefs = status.preferences
  }

  onMount(() => {
    refreshPush().catch(() => undefined)
  })

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

  async function togglePush() {
    pushBusy = true
    try {
      if (pushSubscribed) {
        await disableNotifications()
        pushSubscribed = false
        showToast('Notifications disabled')
      } else {
        await enableNotifications()
        pushSubscribed = true
        showToast(
          isIos
            ? 'Notifications enabled — iOS only delivers when opened from the Home Screen (iOS 16.4+)'
            : 'Notifications enabled',
        )
        try {
          await sendTestNotification()
        } catch {
          /* optional */
        }
      }
      await refreshPush()
    } catch (err) {
      showToast(err instanceof Error ? err.message : 'Push failed')
      await refreshPush()
    } finally {
      pushBusy = false
    }
  }

  async function testPush() {
    pushBusy = true
    try {
      if (!pushSubscribed) {
        await enableNotifications()
        pushSubscribed = true
      }
      await sendTestNotification()
      showToast(isIos ? 'Test sent — check Notification Center' : 'Test notification sent')
    } catch (err) {
      showToast(err instanceof Error ? err.message : 'Test failed')
    } finally {
      pushBusy = false
    }
  }

  async function togglePref(key: keyof NotificationPrefs) {
    const next = { ...prefs, [key]: !prefs[key] }
    prefs = next
    if (!pushSubscribed) return
    prefsBusy = true
    try {
      await saveNotificationPrefs(next)
    } catch (err) {
      showToast(err instanceof Error ? err.message : 'Could not save preference')
      await refreshPush()
    } finally {
      prefsBusy = false
    }
  }
</script>

<div class="account stack">
  <div class="card stack account-card">
    <div>
      <h2>Account</h2>
      <p class="muted">Password and notification preferences.</p>
    </div>
    <form class="stack" onsubmit={submit}>
      <h3>Change password</h3>
      <label>Current password
        <input bind:value={current_password} type="password" required />
      </label>
      <label>New password
        <input bind:value={new_password} type="password" required minlength="8" />
      </label>
      <button type="submit" disabled={loading}>{loading ? 'Saving…' : 'Update password'}</button>
    </form>
  </div>

  <div class="card stack account-card">
    <div>
      <h3>Notifications</h3>
      <p class="muted">
        Status: {pushSubscribed ? 'on' : 'off'}
        {#if needsHttps} · needs HTTPS{/if}
      </p>
    </div>

    {#if needsInstall}
      <div class="install-banner">
        <strong>Install to Home Screen (iOS 16.4+)</strong>
        <p class="muted">
          Safari → Share → Add to Home Screen, then open Audiobooker from the icon to enable push
          notifications. Browser tabs cannot receive them on iPhone/iPad.
        </p>
      </div>
    {/if}

    <div class="push-actions">
      {#if pushSupported || needsInstall}
        <button class="secondary" type="button" disabled={pushBusy || needsInstall} onclick={togglePush}>
          {#if pushBusy}
            Working…
          {:else if pushSubscribed}
            Disable notifications
          {:else}
            Enable notifications
          {/if}
        </button>
        <button class="secondary" type="button" disabled={pushBusy || needsInstall} onclick={testPush}>
          Send test
        </button>
      {:else}
        <p class="muted">Push notifications are not supported in this browser.</p>
      {/if}
    </div>

    <div class="notify-prefs" class:dim={!pushSubscribed}>
      <label class="pref">
        <input
          type="checkbox"
          checked={prefs.notify_imported}
          disabled={prefsBusy || !pushSubscribed}
          onchange={() => togglePref('notify_imported')}
        />
        <span>Book imported into library</span>
      </label>
      <label class="pref">
        <input
          type="checkbox"
          checked={prefs.notify_pack_ready}
          disabled={prefsBusy || !pushSubscribed}
          onchange={() => togglePref('notify_pack_ready')}
        />
        <span>Pack ready to map</span>
      </label>
      <label class="pref">
        <input
          type="checkbox"
          checked={prefs.notify_download_finished}
          disabled={prefsBusy || !pushSubscribed}
          onchange={() => togglePref('notify_download_finished')}
        />
        <span>Download finished (before import)</span>
      </label>
      <label class="pref">
        <input
          type="checkbox"
          checked={prefs.notify_failures}
          disabled={prefsBusy || !pushSubscribed}
          onchange={() => togglePref('notify_failures')}
        />
        <span>Failures</span>
      </label>
    </div>
  </div>
</div>

<style>
  .account {
    gap: 1rem;
    width: 100%;
    max-width: 520px;
  }
  .account-card {
    width: 100%;
    max-width: none;
  }
  .push-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
  }
  .notify-prefs {
    display: grid;
    gap: 0.55rem;
  }
  .notify-prefs.dim {
    opacity: 0.55;
  }
  .pref {
    display: flex;
    align-items: flex-start;
    gap: 0.55rem;
    font-weight: 500;
    line-height: 1.35;
  }
  .pref input {
    margin-top: 0.15rem;
    flex: 0 0 auto;
  }
  .install-banner {
    border: 1px solid color-mix(in oklab, var(--accent) 40%, var(--border));
    border-radius: 10px;
    padding: 0.75rem 0.9rem;
    background: color-mix(in oklab, var(--accent) 8%, transparent);
  }
  .install-banner p {
    margin: 0.35rem 0 0;
  }
  @media (max-width: 520px) {
    .push-actions button {
      flex: 1 1 auto;
      justify-content: center;
      text-align: center;
    }
  }
</style>
