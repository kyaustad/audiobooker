<script lang="ts">
  import { onDestroy, onMount } from 'svelte'
  import { push } from 'svelte-spa-router'
  import { api, type Download } from '../lib/api'
  import { enableNotifications, disableNotifications, getPushStatus } from '../lib/push'
  import { showToast } from '../lib/toast'

  let downloads = $state<Download[]>([])
  let input = $state('')
  let name = $state('')
  let loading = $state(true)
  let submitting = $state(false)
  let pushBusy = $state(false)
  let pushSubscribed = $state(false)
  let pushSupported = $state(true)
  let timer: number | undefined

  async function refresh() {
    const data = await api.listDownloads()
    downloads = data.downloads
  }

  async function refreshPush() {
    const status = await getPushStatus()
    pushSupported = status.supported
    pushSubscribed = status.subscribed && status.permission === 'granted'
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

  async function remove(id: number) {
    try {
      await api.deleteDownload(id)
      showToast('Removed')
      await refresh()
    } catch (err) {
      showToast(err instanceof Error ? err.message : 'Failed to remove')
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
      }
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
</script>

<div class="card">
  <div class="row" style="justify-content:space-between;align-items:start">
    <div>
      <h2>Your queue</h2>
      <p class="muted">Add an info hash or magnet, then match it to Audible metadata before download starts.</p>
    </div>
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
    {/if}
  </div>

  <form class="stack" style="margin-top:1rem" onsubmit={addDownload}>
    <label>Magnet or info hash
      <input bind:value={input} placeholder="magnet:?xt=urn:btih:… or 40-char hash" required />
    </label>
    <label>Display name (optional)
      <input bind:value={name} placeholder="Working title" />
    </label>
    <div class="row">
      <button type="submit" disabled={submitting}>{submitting ? 'Adding…' : 'Add download'}</button>
      <a class="btn secondary" href="#/browse" style="display:inline-flex;align-items:center">Open AudiobookBay</a>
    </div>
  </form>
</div>

<div class="card">
  <h3>Downloads</h3>
  {#if loading}
    <p class="muted">Loading…</p>
  {:else if downloads.length === 0}
    <p class="muted">No downloads yet.</p>
  {:else}
    <div class="download-grid" style="margin-top:0.75rem">
      {#each downloads as d}
        <div class="card download-item" style="margin:0">
          <img src={d.metadata?.cover_url || '/favicon.svg'} alt="" />
          <div>
            <strong>{d.metadata?.title || d.name || 'Untitled'}</strong>
            {#if d.metadata?.authors?.length}
              <div class="muted">{d.metadata.authors.join(', ')}</div>
            {/if}
            <div style="margin:0.45rem 0">
              <span class={`badge ${d.status}`}>{d.status}</span>
            </div>
            <div class="progress"><span style={`width:${Math.round(d.progress * 100)}%`}></span></div>
            <div class="muted" style="margin-top:0.35rem">
              {Math.round(d.progress * 100)}% · {formatBytes(d.download_speed)}
            </div>
            {#if d.destination_path}
              <div class="muted">Imported to {d.destination_path}</div>
            {/if}
            {#if d.error_message}
              <div style="color:var(--danger);font-size:0.85rem">{d.error_message}</div>
            {/if}
          </div>
          <div class="actions stack">
            {#if d.status === 'awaiting_match'}
              <a class="btn" href={`#/match/${d.id}`}>Match</a>
            {/if}
            <button class="danger" type="button" onclick={() => remove(d.id)}>Remove</button>
          </div>
        </div>
      {/each}
    </div>
  {/if}
</div>
