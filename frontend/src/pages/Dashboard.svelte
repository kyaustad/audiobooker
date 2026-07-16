<script lang="ts">
  import { onDestroy, onMount } from 'svelte'
  import { push } from 'svelte-spa-router'
  import { api, type Download, type NotificationPrefs } from '../lib/api'
  import {
    enableNotifications,
    disableNotifications,
    getPushStatus,
    saveNotificationPrefs,
    sendTestNotification,
  } from '../lib/push'
  import { showToast } from '../lib/toast'

  type Tab = 'all' | 'matching' | 'active' | 'completed' | 'failed'

  let downloads = $state<Download[]>([])
  let input = $state('')
  let name = $state('')
  let loading = $state(true)
  let submitting = $state(false)
  let pushBusy = $state(false)
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
  let prefsBusy = $state(false)
  let tab = $state<Tab>('all')
  let removingId = $state<number | null>(null)
  let refreshingId = $state<number | null>(null)
  let timer: number | undefined

  const SEEDING_STATUSES = new Set(['completed', 'copying', 'imported', 'awaiting_map', 'partial'])

  function tabFor(status: string): Tab {
    if (status === 'awaiting_match' || status === 'awaiting_map') return 'matching'
    if (status === 'error') return 'failed'
    if (status === 'imported' || status === 'partial' || status === 'completed' || status === 'copying') {
      return 'completed'
    }
    return 'active'
  }

  function canRemove(d: Download) {
    if (SEEDING_STATUSES.has(d.status)) return false
    if ((d.items || []).some((i) => i.status === 'imported')) return false
    return true
  }

  function isPack(d: Download) {
    return (d.kind || 'single') === 'pack'
  }

  function packProgress(d: Download) {
    const items = d.items || []
    if (!items.length) return null
    const imported = items.filter((i) => i.status === 'imported').length
    return `${imported}/${items.length} mapped books imported`
  }

  const counts = $derived({
    all: downloads.length,
    matching: downloads.filter((d) => tabFor(d.status) === 'matching').length,
    active: downloads.filter((d) => tabFor(d.status) === 'active').length,
    completed: downloads.filter((d) => tabFor(d.status) === 'completed').length,
    failed: downloads.filter((d) => tabFor(d.status) === 'failed').length,
  })

  const visible = $derived(
    tab === 'all' ? downloads : downloads.filter((d) => tabFor(d.status) === tab),
  )

  async function refresh() {
    const data = await api.listDownloads()
    downloads = data.downloads
  }

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
    refresh()
      .catch((e) => showToast(e.message))
      .finally(() => (loading = false))
    refreshPush().catch(() => undefined)
    timer = window.setInterval(() => {
      refresh().catch(() => undefined)
    }, 8000)
  })

  onDestroy(() => {
    if (timer) clearInterval(timer)
  })

  async function addDownload(e: Event) {
    e.preventDefault()
    submitting = true
    try {
      const { download } = await api.createDownload(input, name || undefined)
      showToast('Added — match Audible metadata next')
      input = ''
      name = ''
      push(`/match/${download.id}`)
    } catch (err) {
      showToast(err instanceof Error ? err.message : 'Failed to add')
    } finally {
      submitting = false
    }
  }

  async function remove(d: Download) {
    if (!canRemove(d)) {
      showToast('Completed downloads stay in the queue to seed. qBittorrent removes them at your ratio limit.')
      return
    }
    const label = d.metadata?.title || d.name || 'this download'
    if (!window.confirm(`Remove “${label}” from the queue?\n\nThis also removes it from qBittorrent but keeps downloaded files.`)) {
      return
    }
    removingId = d.id
    try {
      await api.deleteDownload(d.id)
      showToast('Removed from queue')
      await refresh()
    } catch (err) {
      showToast(err instanceof Error ? err.message : 'Failed to remove')
    } finally {
      removingId = null
    }
  }

  async function refreshQbit(d: Download) {
    refreshingId = d.id
    try {
      const data = await api.refreshQbittorrent(d.id)
      showToast(
        data.paths_changed
          ? `Updated paths from qBit${data.requeued_items ? ` · requeued ${data.requeued_items}` : ''}`
          : `Synced from qBit (${data.qb_state})${data.requeued_items ? ` · requeued ${data.requeued_items}` : ''}`,
      )
      await refresh()
    } catch (err) {
      showToast(err instanceof Error ? err.message : 'qBit refresh failed')
    } finally {
      refreshingId = null
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
        showToast('Notifications enabled')
        try {
          await sendTestNotification()
        } catch {
          /* optional */
        }
      }
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
      await refreshPush()
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
      showToast(err instanceof Error ? err.message : 'Could not save notification settings')
      await refreshPush()
    } finally {
      prefsBusy = false
    }
  }

  function formatBytes(n: number) {
    if (n <= 0) return '0 B/s'
    const u = ['B/s', 'KB/s', 'MB/s', 'GB/s']
    const i = Math.min(Math.floor(Math.log(n) / Math.log(1024)), u.length - 1)
    return `${(n / 1024 ** i).toFixed(1)} ${u[i]}`
  }

  function statusLabel(status: string) {
    return status.replaceAll('_', ' ')
  }
</script>

<div class="card queue-hero">
  <div class="queue-hero-top">
    <div>
      <h2>Your queue</h2>
      <p class="muted hide-mobile">
        Add an info hash or magnet, then match Audible metadata before download starts.
      </p>
      <p class="muted push-line">
        Notifications: {pushSubscribed ? 'on' : 'off'}
        {#if needsHttps}
          · require HTTPS
        {/if}
      </p>
    </div>
    <div class="push-actions">
      {#if pushSupported}
        <button class="secondary" type="button" disabled={pushBusy} onclick={togglePush}>
          {#if pushBusy}
            Working…
          {:else if pushSubscribed}
            Notifications on
          {:else}
            Enable notifications
          {/if}
        </button>
        <button class="secondary" type="button" disabled={pushBusy} onclick={testPush}>
          Send test
        </button>
      {:else if needsInstall}
        <span class="muted tiny">Install to Home Screen to enable</span>
      {/if}
    </div>
  </div>

  {#if needsInstall}
    <div class="banner-warn ios-hint">
      On iPhone/iPad, open Safari → Share → <strong>Add to Home Screen</strong>, then launch
      Audiobooker from the home-screen icon (not a Safari tab) to enable notifications.
    </div>
  {/if}

  {#if pushSupported || pushSubscribed}
    <div class="notify-prefs" class:dim={!pushSubscribed}>
      <p class="muted tiny prefs-label">Notify me when</p>
      <label class="pref">
        <input
          type="checkbox"
          checked={prefs.notify_imported}
          disabled={prefsBusy || !pushSubscribed}
          onchange={() => togglePref('notify_imported')}
        />
        Ready in library
      </label>
      <label class="pref">
        <input
          type="checkbox"
          checked={prefs.notify_pack_ready}
          disabled={prefsBusy || !pushSubscribed}
          onchange={() => togglePref('notify_pack_ready')}
        />
        Pack ready to map
      </label>
      <label class="pref">
        <input
          type="checkbox"
          checked={prefs.notify_download_finished}
          disabled={prefsBusy || !pushSubscribed}
          onchange={() => togglePref('notify_download_finished')}
        />
        Download finished (before import)
      </label>
      <label class="pref">
        <input
          type="checkbox"
          checked={prefs.notify_failures}
          disabled={prefsBusy || !pushSubscribed}
          onchange={() => togglePref('notify_failures')}
        />
        Failures
      </label>
    </div>
  {/if}

  <form class="stack add-form" onsubmit={addDownload}>
    <label>Magnet or info hash
      <input bind:value={input} placeholder="magnet:?xt=urn:btih:… or 40-char hash" required />
    </label>
    <label>Display name (optional)
      <input bind:value={name} placeholder="Working title" />
    </label>
    <div class="add-actions">
      <button type="submit" disabled={submitting}>{submitting ? 'Adding…' : 'Add download'}</button>
      <a class="btn secondary" href="#/browse">Discover</a>
    </div>
  </form>
</div>

<div class="card downloads-panel">
  <h3>Downloads</h3>
  <div class="status-tabs" role="tablist" aria-label="Download status">
    {#each [
      ['all', 'All'],
      ['matching', 'Matching'],
      ['active', 'Active'],
      ['completed', 'Completed'],
      ['failed', 'Failed'],
    ] as [id, label]}
      <button
        type="button"
        role="tab"
        class="tab"
        class:active={tab === id}
        aria-selected={tab === id}
        onclick={() => (tab = id as Tab)}
      >
        {label}
        <span class="count">{counts[id as Tab]}</span>
      </button>
    {/each}
  </div>

  {#if loading}
    <p class="muted">Loading…</p>
  {:else if visible.length === 0}
    <p class="muted">
      {#if downloads.length === 0}
        No downloads yet.
      {:else}
        Nothing in this tab.
      {/if}
    </p>
  {:else}
    <div class="download-grid">
      {#each visible as d}
        <article class="download-item">
          <img class="cover" src={d.metadata?.cover_url || '/favicon.svg'} alt="" />
          <div class="meta">
            <strong class="title">{d.metadata?.title || d.name || 'Untitled'}</strong>
            {#if d.metadata?.authors?.length}
              <div class="muted author">{d.metadata.authors.join(', ')}</div>
            {/if}
            <div class="badges">
              <span class={`badge ${d.status}`}>{statusLabel(d.status)}</span>
              {#if isPack(d)}
                <span class="badge pack">pack</span>
              {/if}
            </div>
            <div class="progress"><span style={`width:${Math.round(d.progress * 100)}%`}></span></div>
            <div class="muted stats">
              {Math.round(d.progress * 100)}% · {formatBytes(d.download_speed)}
            </div>
            {#if packProgress(d)}
              <div class="muted">{packProgress(d)}</div>
            {/if}
            {#if d.destination_path}
              <div class="muted dest">Imported to {d.destination_path}</div>
            {/if}
            {#if SEEDING_STATUSES.has(d.status)}
              <div class="muted seeding-note">Seeding in qBittorrent — left locked until ratio rules drop it.</div>
            {/if}
            {#if d.error_message}
              <div class="err">{d.error_message}</div>
            {/if}
          </div>
          <div class="actions">
            {#if d.status === 'awaiting_match'}
              <a class="btn" href={`#/match/${d.id}`}>Match</a>
            {:else if isPack(d) && d.status !== 'awaiting_match'}
              <a class="btn" href={`#/map/${d.id}`}>
                {d.status === 'imported' || d.status === 'partial' ? 'Map more' : 'Map'}
              </a>
            {/if}
            {#if d.status !== 'awaiting_match'}
              <button
                class="secondary"
                type="button"
                disabled={refreshingId === d.id}
                onclick={() => refreshQbit(d)}
              >
                {refreshingId === d.id ? '…' : 'Refresh qBit'}
              </button>
            {/if}
            {#if canRemove(d)}
              <button class="danger" type="button" disabled={removingId === d.id} onclick={() => remove(d)}>
                {removingId === d.id ? '…' : 'Remove'}
              </button>
            {:else}
              <span class="muted tiny">Seeding</span>
            {/if}
          </div>
        </article>
      {/each}
    </div>
  {/if}
</div>

<style>
  .queue-hero-top {
    display: flex;
    justify-content: space-between;
    gap: 0.75rem;
    align-items: start;
    flex-wrap: wrap;
  }
  .push-actions, .add-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
  }
  .add-form {
    margin-top: 1rem;
  }
  .push-line {
    margin-top: 0.35rem;
  }
  .notify-prefs {
    display: flex;
    flex-wrap: wrap;
    gap: 0.55rem 1rem;
    align-items: center;
    margin-top: 0.85rem;
    padding-top: 0.75rem;
    border-top: 1px solid var(--border);
  }
  .notify-prefs.dim {
    opacity: 0.55;
  }
  .prefs-label {
    width: 100%;
    margin: 0;
  }
  .pref {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    color: var(--text);
    font-size: 0.88rem;
    flex: 0 0 auto;
  }
  .ios-hint {
    margin-top: 0.75rem;
  }
  .banner-warn {
    background: color-mix(in oklab, var(--warning) 16%, transparent);
    border: 1px solid color-mix(in oklab, var(--warning) 45%, var(--border));
    border-radius: 8px;
    padding: 0.7rem 0.85rem;
    font-size: 0.9rem;
  }
  .status-tabs {
    display: flex;
    flex-wrap: nowrap;
    gap: 0.4rem;
    margin: 0.75rem 0 0.85rem;
    overflow-x: auto;
    -webkit-overflow-scrolling: touch;
    scrollbar-width: none;
    padding-bottom: 0.15rem;
  }
  .status-tabs::-webkit-scrollbar {
    display: none;
  }
  .status-tabs .tab {
    background: transparent !important;
    color: var(--muted) !important;
    border: 1px solid var(--border) !important;
    border-radius: 999px;
    padding: 0.4rem 0.8rem !important;
    font-weight: 600;
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
    flex: 0 0 auto;
    white-space: nowrap;
  }
  .status-tabs .tab.active,
  .status-tabs .tab:hover {
    color: var(--text) !important;
    border-color: var(--accent) !important;
    background: color-mix(in oklab, var(--accent) 12%, transparent) !important;
  }
  .count {
    font-size: 0.75rem;
    font-weight: 700;
    color: var(--muted);
    background: var(--bg);
    border-radius: 999px;
    padding: 0.05rem 0.4rem;
  }
  .tab.active .count {
    color: var(--accent);
  }
  .download-grid {
    display: grid;
    gap: 0.75rem;
  }
  .download-item {
    display: grid;
    grid-template-columns: 72px 1fr auto;
    gap: 0.75rem 0.85rem;
    align-items: start;
    padding: 0.85rem;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--bg);
  }
  .cover {
    width: 72px;
    height: 72px;
    object-fit: cover;
    border-radius: 8px;
    background: var(--bg-elevated);
    border: 1px solid var(--border);
  }
  .title {
    display: -webkit-box;
    -webkit-line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
    line-height: 1.25;
  }
  .author {
    margin-top: 0.15rem;
  }
  .badges {
    display: flex;
    flex-wrap: wrap;
    gap: 0.3rem;
    margin: 0.45rem 0 0.4rem;
  }
  .stats {
    margin-top: 0.35rem;
  }
  .dest {
    margin-top: 0.25rem;
    word-break: break-all;
    font-size: 0.8rem;
  }
  .seeding-note {
    margin-top: 0.35rem;
    font-size: 0.82rem;
  }
  .err {
    color: var(--danger);
    font-size: 0.85rem;
    margin-top: 0.35rem;
  }
  .actions {
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
    align-items: stretch;
    min-width: 5.5rem;
  }
  .actions .btn,
  .actions button {
    text-align: center;
    justify-content: center;
  }
  .tiny {
    font-size: 0.78rem;
  }
  .hide-mobile {
    display: block;
  }
  @media (max-width: 640px) {
    .hide-mobile {
      display: none;
    }
    .download-item {
      grid-template-columns: 64px 1fr;
      grid-template-areas:
        'cover meta'
        'actions actions';
    }
    .cover {
      grid-area: cover;
      width: 64px;
      height: 64px;
    }
    .meta {
      grid-area: meta;
      min-width: 0;
    }
    .actions {
      grid-area: actions;
      flex-direction: row;
      flex-wrap: wrap;
      min-width: 0;
    }
    .actions .btn,
    .actions button {
      flex: 1 1 auto;
      min-width: 0;
      padding: 0.45rem 0.65rem;
      font-size: 0.9rem;
    }
    .push-actions {
      width: 100%;
    }
    .push-actions button {
      flex: 1 1 auto;
    }
    .add-actions {
      flex-direction: column;
    }
    .add-actions > * {
      width: 100%;
      text-align: center;
      justify-content: center;
    }
  }
</style>
