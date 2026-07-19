<script lang="ts">
  import { onDestroy, onMount } from 'svelte'
  import { push } from 'svelte-spa-router'
  import { api, type Download, type User } from '../lib/api'
  import { canStartDownloads, isRequester } from '../lib/roles'
  import { enableNotifications, getPushStatus } from '../lib/push'
  import { currentUser } from '../lib/session'
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
  let tab = $state<Tab>('all')
  let removingId = $state<number | null>(null)
  let removeTarget = $state<Download | null>(null)
  let removeDeleteFiles = $state(false)
  let refreshingId = $state<number | null>(null)
  let retryingId = $state<number | null>(null)
  let user = $state<User | null>(null)
  let timer: number | undefined

  currentUser.subscribe((v) => (user = v))

  const SEEDING_STATUSES = new Set(['completed', 'copying', 'imported', 'awaiting_map', 'partial'])

  function tabFor(status: string): Tab {
    if (
      status === 'awaiting_match' ||
      status === 'awaiting_map' ||
      status === 'pending_approval'
    ) {
      return 'matching'
    }
    if (status === 'error' || status === 'rejected') return 'failed'
    if (status === 'imported' || status === 'partial' || status === 'completed' || status === 'copying') {
      return 'completed'
    }
    return 'active'
  }

  function canRemove(d: Download) {
    if (SEEDING_STATUSES.has(d.status)) return false
    if ((d.items || []).some((i) => i.status === 'imported')) return false
    const preDownload = ['awaiting_match', 'pending_approval', 'rejected'].includes(d.status)
    if (preDownload) return true
    return user?.can_remove !== false
  }

  function isPreDownload(d: Download) {
    return ['awaiting_match', 'pending_approval', 'rejected'].includes(d.status)
  }

  function removeLabel(d: Download) {
    return d.metadata?.title || d.name || 'this download'
  }

  function isPack(d: Download) {
    return (d.kind || 'single') === 'pack'
  }

  function usesMapping(d: Download) {
    return isPack(d) || Boolean(d.map_files)
  }

  /** Refresh qBit is for mapped downloads (path moves / remapping), not finished singles. */
  function showRefreshQbit(d: Download) {
    if (!usesMapping(d) || !canStartDownloads(user)) return false
    if (d.status === 'awaiting_match' || d.status === 'pending_approval') return false
    if (d.status === 'imported') {
      const items = d.items || []
      return items.some((i) => i.status === 'error' || i.status === 'ready' || i.status === 'pending')
    }
    return true
  }

  function showMap(d: Download) {
    if (!canStartDownloads(user)) return false
    return usesMapping(d) && !['awaiting_match', 'pending_approval', 'rejected'].includes(d.status)
  }

  /** Single-book stuck copy or failed import after metadata match. */
  function showRetryImport(d: Download) {
    if (!canStartDownloads(user)) return false
    if (usesMapping(d)) return false
    if (d.status === 'copying') return true
    return d.status === 'error' && Boolean(d.metadata)
  }

  function showReimport(d: Download) {
    return canStartDownloads(user) && !usesMapping(d) && d.status === 'imported' && Boolean(d.metadata)
  }

  function showUnimport(d: Download) {
    return (
      canStartDownloads(user) &&
      !usesMapping(d) &&
      (d.status === 'imported' || (d.status === 'error' && Boolean(d.destination_path))) &&
      Boolean(d.metadata)
    )
  }

  function hasActions(d: Download) {
    return (
      d.status === 'awaiting_match' ||
      d.status === 'rejected' ||
      showMap(d) ||
      showRefreshQbit(d) ||
      showRetryImport(d) ||
      showReimport(d) ||
      showUnimport(d) ||
      canRemove(d)
    )
  }

  function shortPath(path: string | null | undefined) {
    if (!path) return ''
    const parts = path.split('/').filter(Boolean)
    if (parts.length <= 3) return path
    return `…/${parts.slice(-3).join('/')}`
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

  function openRemoveDialog(d: Download) {
    if (!canRemove(d)) {
      showToast('Completed downloads stay in the queue to seed. qBittorrent removes them at your ratio limit.')
      return
    }
    removeTarget = d
    removeDeleteFiles = false
  }

  function closeRemoveDialog() {
    if (removingId != null) return
    removeTarget = null
    removeDeleteFiles = false
  }

  async function confirmRemove() {
    const d = removeTarget
    if (!d) return
    const deleteFiles = !isPreDownload(d) && Boolean(user?.can_remove_files) && removeDeleteFiles
    removingId = d.id
    try {
      await api.deleteDownload(d.id, deleteFiles)
      showToast(deleteFiles ? 'Removed (files deleted)' : 'Removed from queue')
      removeTarget = null
      removeDeleteFiles = false
      await refresh()
    } catch (err) {
      showToast(err instanceof Error ? err.message : 'Failed to remove')
    } finally {
      removingId = null
    }
  }

  async function unimportSingle(d: Download) {
    const label = d.metadata?.title || d.name || 'this book'
    if (
      !window.confirm(
        `Un-import “${label}”?\n\nDeletes the library copy so you can map the correct files from the download.`,
      )
    ) {
      return
    }
    retryingId = d.id
    try {
      await api.unimportDownload(d.id)
      showToast('Un-imported — map files next')
      push(`/map/${d.id}`)
    } catch (err) {
      showToast(err instanceof Error ? err.message : 'Un-import failed')
    } finally {
      retryingId = null
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

  async function retryImport(d: Download) {
    if (
      d.status === 'copying' &&
      !window.confirm(
        'Retry this import?\n\nOnly use if the copy looks stuck (for example after a restart).',
      )
    ) {
      return
    }
    retryingId = d.id
    try {
      await api.retryImport(d.id)
      showToast('Import queued again')
      await refresh()
    } catch (err) {
      showToast(err instanceof Error ? err.message : 'Retry failed')
    } finally {
      retryingId = null
    }
  }

  async function reimport(d: Download) {
    const label = d.metadata?.title || d.name || 'this book'
    if (
      !window.confirm(
        `Re-import “${label}”?\n\nThis replaces the files in your library folder with a fresh copy from the download.`,
      )
    ) {
      return
    }
    retryingId = d.id
    try {
      await api.reimportDownload(d.id)
      showToast('Re-import queued — overwriting library copy')
      await refresh()
    } catch (err) {
      showToast(err instanceof Error ? err.message : 'Re-import failed')
    } finally {
      retryingId = null
    }
  }

  async function togglePush() {
    pushBusy = true
    try {
      await enableNotifications()
      pushSubscribed = true
      showToast(
        isIos
          ? 'Notifications enabled — delivery requires Home Screen app (iOS 16.4+)'
          : 'Notifications enabled',
      )
      await refreshPush()
    } catch (err) {
      showToast(err instanceof Error ? err.message : 'Push failed')
      await refreshPush()
    } finally {
      pushBusy = false
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
        {#if isRequester(user)}
          Add a magnet or info hash, match Audible, then wait for an approver to start the download.
        {:else}
          Add an info hash or magnet, then match Audible metadata before download starts.
        {/if}
      </p>
      <p class="muted push-line">
        Notifications: {pushSubscribed ? 'on' : 'off'}
        {#if needsHttps}
          · require HTTPS
        {/if}
      </p>
    </div>
    <div class="push-actions">
      {#if pushSupported && !pushSubscribed}
        <button class="secondary" type="button" disabled={pushBusy || needsInstall} onclick={togglePush}>
          {pushBusy ? 'Working…' : 'Enable notifications'}
        </button>
      {/if}
      <a class="btn secondary" href="#/account">Notification settings</a>
    </div>
  </div>

  {#if needsInstall}
    <div class="banner-warn ios-hint">
      On iPhone/iPad (iOS 16.4+), open Safari → Share → <strong>Add to Home Screen</strong>, then launch
      Audiobooker from the home-screen icon (not a Safari tab) to enable notifications.
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
      ['matching', 'Waiting'],
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
        <article class="download-item" class:done={d.status === 'imported' && !isPack(d)}>
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
              {:else if d.map_files}
                <span class="badge pack">map files</span>
              {/if}
              {#if d.status === 'imported' && !usesMapping(d)}
                <span class="badge seeding-pill">seeding</span>
              {/if}
            </div>
            {#if d.status !== 'imported'}
              <div class="progress"><span style={`width:${Math.round(d.progress * 100)}%`}></span></div>
              <div class="muted stats">
                {Math.round(d.progress * 100)}%
                {#if d.download_speed > 0}
                  · {formatBytes(d.download_speed)}
                {/if}
              </div>
            {/if}
            {#if packProgress(d)}
              <div class="muted pack-note">{packProgress(d)}</div>
            {/if}
            {#if d.destination_path && d.status === 'imported'}
              <div class="muted dest" title={d.destination_path}>{shortPath(d.destination_path)}</div>
            {/if}
            {#if d.error_message}
              <div class="err">{d.error_message}</div>
            {/if}
          </div>
          {#if hasActions(d)}
            <div class="actions">
              {#if d.status === 'awaiting_match' || d.status === 'rejected'}
                <a class="btn" href={`#/match/${d.id}`}>Match</a>
              {/if}
              {#if showMap(d)}
                <a class="btn primary-action" href={`#/map/${d.id}`}>
                  {d.status === 'imported' || d.status === 'partial' ? 'Map more' : 'Map'}
                </a>
              {/if}
              {#if showRetryImport(d)}
                <button
                  class="secondary"
                  type="button"
                  disabled={retryingId === d.id}
                  onclick={() => retryImport(d)}
                >
                  {#if retryingId === d.id}
                    …
                  {:else if d.status === 'copying'}
                    <span class="hide-mobile">Reset stuck copy</span>
                    <span class="show-mobile">Reset copy</span>
                  {:else}
                    Retry import
                  {/if}
                </button>
              {/if}
              {#if showReimport(d)}
                <button
                  class="secondary"
                  type="button"
                  disabled={retryingId === d.id}
                  onclick={() => reimport(d)}
                >
                  {retryingId === d.id ? '…' : 'Re-import'}
                </button>
              {/if}
              {#if showUnimport(d)}
                <button
                  class="secondary"
                  type="button"
                  disabled={retryingId === d.id}
                  onclick={() => unimportSingle(d)}
                >
                  {retryingId === d.id ? '…' : 'Un-import → Map'}
                </button>
              {/if}
              {#if showRefreshQbit(d)}
                <button
                  class="secondary"
                  type="button"
                  disabled={refreshingId === d.id}
                  onclick={() => refreshQbit(d)}
                >
                  {#if refreshingId === d.id}
                    …
                  {:else}
                    <span class="hide-mobile">Refresh qBit</span>
                    <span class="show-mobile">Refresh</span>
                  {/if}
                </button>
              {/if}
              {#if canRemove(d)}
                <button class="danger" type="button" disabled={removingId === d.id} onclick={() => openRemoveDialog(d)}>
                  {removingId === d.id ? '…' : 'Remove'}
                </button>
              {/if}
            </div>
          {/if}
        </article>
      {/each}
    </div>
  {/if}
</div>

{#if removeTarget}
  <div
    class="modal-backdrop"
    role="presentation"
    onclick={closeRemoveDialog}
    onkeydown={(e) => e.key === 'Escape' && closeRemoveDialog()}
  >
    <div
      class="modal"
      role="dialog"
      aria-modal="true"
      aria-labelledby="remove-dialog-title"
      tabindex="-1"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.stopPropagation()}
    >
      <h3 id="remove-dialog-title">Remove download?</h3>
      <p class="modal-title">{removeLabel(removeTarget)}</p>

      {#if isPreDownload(removeTarget)}
        <p class="muted">
          This removes the request from your queue. Nothing has been sent to qBittorrent yet.
        </p>
      {:else}
        <p class="muted">
          This removes the item from your Audiobooker queue and from qBittorrent.
        </p>
        {#if user?.can_remove_files}
          <label class="remove-option">
            <input type="checkbox" bind:checked={removeDeleteFiles} disabled={removingId != null} />
            <span>
              <strong>Also delete downloaded files</strong>
              <span class="muted">
                {#if removeDeleteFiles}
                  Removes the torrent and deletes its data from disk.
                {:else}
                  Removes the torrent from qBittorrent but keeps files on disk.
                {/if}
              </span>
            </span>
          </label>
        {:else}
          <p class="muted keep-note">Downloaded files on disk will be kept.</p>
        {/if}
      {/if}

      <div class="modal-actions">
        <button class="secondary" type="button" disabled={removingId != null} onclick={closeRemoveDialog}>
          Cancel
        </button>
        <button class="danger" type="button" disabled={removingId != null} onclick={confirmRemove}>
          {#if removingId != null}
            Removing…
          {:else if !isPreDownload(removeTarget) && removeDeleteFiles}
            Remove + delete files
          {:else}
            Remove
          {/if}
        </button>
      </div>
    </div>
  </div>
{/if}

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
    line-clamp: 2;
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
    margin-top: 0.35rem;
    font-size: 0.78rem;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .pack-note {
    margin-top: 0.25rem;
    font-size: 0.82rem;
  }
  .seeding-pill {
    background: color-mix(in oklab, var(--muted) 18%, transparent);
    color: var(--muted);
    border-color: var(--border);
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
    min-width: 7.25rem;
  }
  .actions:empty {
    display: none;
  }
  .actions .btn,
  .actions button {
    text-align: center;
    justify-content: center;
    white-space: nowrap;
    padding-left: 0.65rem;
    padding-right: 0.65rem;
  }
  .download-item.done {
    opacity: 0.92;
  }
  .tiny {
    font-size: 0.78rem;
  }
  .hide-mobile {
    display: block;
  }
  .show-mobile {
    display: none;
  }
  @media (max-width: 640px) {
    .hide-mobile {
      display: none;
    }
    .show-mobile {
      display: inline;
    }
    .download-item {
      grid-template-columns: 56px 1fr;
      grid-template-areas:
        'cover meta'
        'actions actions';
      gap: 0.55rem 0.7rem;
      padding: 0.75rem;
    }
    .cover {
      grid-area: cover;
      width: 56px;
      height: 56px;
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
      padding-top: 0.55rem;
      margin-top: 0.1rem;
      border-top: 1px solid var(--border);
    }
    .actions .btn,
    .actions button {
      flex: 1 1 calc(50% - 0.25rem);
      min-width: calc(50% - 0.25rem);
      padding: 0.55rem 0.5rem;
      font-size: 0.86rem;
    }
    .actions .btn.primary-action {
      flex: 1 1 100%;
    }
    .stats {
      font-size: 0.8rem;
    }
    .dest {
      white-space: normal;
      display: -webkit-box;
      -webkit-line-clamp: 2;
      line-clamp: 2;
      -webkit-box-orient: vertical;
    }
    .push-actions {
      width: 100%;
    }
    .push-actions .btn,
    .push-actions button,
    .push-actions a.btn {
      flex: 1 1 auto;
      text-align: center;
      justify-content: center;
    }
    .err {
      word-break: break-word;
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

  .modal-backdrop {
    position: fixed;
    inset: 0;
    z-index: 60;
    display: grid;
    place-items: center;
    padding: 1rem;
    padding-bottom: calc(1rem + env(safe-area-inset-bottom, 0px));
    background: rgba(4, 8, 14, 0.72);
    backdrop-filter: blur(4px);
  }
  .modal {
    width: min(420px, 100%);
    max-height: min(90vh, 560px);
    overflow: auto;
    background: color-mix(in oklab, var(--bg-elevated) 96%, black);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 1.15rem 1.2rem 1.2rem;
    display: grid;
    gap: 0.75rem;
    box-shadow: 0 18px 50px rgba(0, 0, 0, 0.45);
  }
  .modal h3 {
    margin: 0;
    font-size: 1.1rem;
    letter-spacing: -0.02em;
  }
  .modal-title {
    margin: 0;
    font-weight: 600;
    line-height: 1.3;
    word-break: break-word;
  }
  .modal .muted {
    margin: 0;
  }
  .keep-note {
    font-size: 0.88rem;
  }
  .remove-option {
    display: flex !important;
    flex-direction: row !important;
    align-items: flex-start;
    gap: 0.7rem;
    margin: 0;
    padding: 0.8rem 0.85rem;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--bg);
    color: var(--text);
    cursor: pointer;
  }
  .remove-option input {
    margin-top: 0.2rem;
  }
  .remove-option span {
    display: grid;
    gap: 0.2rem;
  }
  .remove-option strong {
    font-size: 0.92rem;
  }
  .modal-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
    justify-content: flex-end;
    margin-top: 0.25rem;
  }
  .modal-actions button {
    min-width: 6.5rem;
  }
  @media (max-width: 640px) {
    .modal-actions {
      flex-direction: column-reverse;
    }
    .modal-actions button {
      width: 100%;
    }
  }
</style>
