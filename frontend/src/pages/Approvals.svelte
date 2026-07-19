<script lang="ts">
  import { onMount } from 'svelte'
  import { api, type Download } from '../lib/api'
  import { showToast } from '../lib/toast'

  let downloads = $state<Download[]>([])
  let loading = $state(true)
  let busyId = $state<number | null>(null)
  let rejectingId = $state<number | null>(null)
  let rejectReason = $state('')

  async function refresh() {
    const data = await api.listPendingDownloads()
    downloads = data.downloads
  }

  onMount(() => {
    refresh()
      .catch((e) => showToast(e.message))
      .finally(() => (loading = false))
  })

  async function approve(d: Download) {
    busyId = d.id
    rejectingId = null
    try {
      await api.approveDownload(d.id)
      showToast('Approved — download started')
      await refresh()
    } catch (err) {
      showToast(err instanceof Error ? err.message : 'Approve failed')
    } finally {
      busyId = null
    }
  }

  function startReject(d: Download) {
    rejectingId = d.id
    rejectReason = ''
  }

  async function confirmReject(d: Download) {
    busyId = d.id
    try {
      await api.rejectDownload(d.id, rejectReason.trim() || undefined)
      showToast('Request rejected')
      rejectingId = null
      rejectReason = ''
      await refresh()
    } catch (err) {
      showToast(err instanceof Error ? err.message : 'Reject failed')
    } finally {
      busyId = null
    }
  }
</script>

<div class="card stack page-hero">
  <div class="page-hero-copy">
    <h2>Approvals</h2>
    <p class="muted">
      Requesters match Audible first. Approving starts the torrent in qBittorrent under their account
      (category <code>audiobooks</code>, tagged with their username).
    </p>
  </div>
</div>

{#if loading}
  <div class="card muted">Loading pending requests…</div>
{:else if !downloads.length}
  <div class="card empty-state">
    <strong>All clear</strong>
    <p class="muted">No requests are waiting for approval.</p>
  </div>
{:else}
  <div class="download-grid">
    {#each downloads as d}
      <article class="download-item">
        <img class="cover" src={d.metadata?.cover_url || '/favicon.svg'} alt="" />
        <div class="meta">
          <strong class="title">{d.metadata?.title || d.name || 'Untitled'}</strong>
          {#if d.metadata?.authors?.length}
            <div class="muted author">{d.metadata.authors.join(', ')}</div>
          {/if}
          <div class="requester muted">
            Requested by <span class="who">{d.username || 'unknown'}</span>
          </div>
          <div class="badges">
            <span class="badge pending_approval">pending approval</span>
            {#if (d.kind || 'single') === 'pack'}
              <span class="badge pack">pack</span>
            {:else if d.map_files}
              <span class="badge pack">map files</span>
            {:else}
              <span class="badge">single</span>
            {/if}
          </div>
        </div>
        <div class="actions">
          {#if rejectingId === d.id}
            <label class="reject-field">
              Reason <span class="muted">(optional)</span>
              <input
                bind:value={rejectReason}
                placeholder="Not needed / wrong match…"
                disabled={busyId === d.id}
              />
            </label>
            <button
              class="danger"
              type="button"
              disabled={busyId === d.id}
              onclick={() => confirmReject(d)}
            >
              {busyId === d.id ? '…' : 'Confirm reject'}
            </button>
            <button
              class="secondary"
              type="button"
              disabled={busyId === d.id}
              onclick={() => (rejectingId = null)}
            >
              Cancel
            </button>
          {:else}
            <button type="button" disabled={busyId === d.id} onclick={() => approve(d)}>
              {busyId === d.id ? '…' : 'Approve'}
            </button>
            <button
              class="danger"
              type="button"
              disabled={busyId === d.id}
              onclick={() => startReject(d)}
            >
              Reject
            </button>
          {/if}
        </div>
      </article>
    {/each}
  </div>
{/if}

<style>
  .page-hero {
    margin-bottom: 0.85rem;
  }
  .page-hero-copy h2 {
    margin-bottom: 0.35rem;
  }
  .page-hero-copy code {
    font-family: var(--mono);
    font-size: 0.85em;
    color: var(--accent);
  }
  .empty-state {
    text-align: center;
    padding: 2rem 1.25rem;
  }
  .empty-state strong {
    display: block;
    margin-bottom: 0.35rem;
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
    background: color-mix(in oklab, var(--bg-elevated) 92%, black);
  }
  .cover {
    width: 72px;
    height: 72px;
    object-fit: cover;
    border-radius: 8px;
    background: var(--bg);
    border: 1px solid var(--border);
  }
  .meta {
    min-width: 0;
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
  .requester {
    margin-top: 0.35rem;
    font-size: 0.88rem;
  }
  .who {
    color: var(--text);
    font-weight: 600;
  }
  .badges {
    display: flex;
    flex-wrap: wrap;
    gap: 0.3rem;
    margin-top: 0.45rem;
  }
  .actions {
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
    align-items: stretch;
    min-width: 8.5rem;
  }
  .reject-field {
    min-width: min(100%, 14rem);
    grid-column: 1 / -1;
  }
  @media (max-width: 640px) {
    .download-item {
      grid-template-columns: 56px 1fr;
      padding: 0.75rem;
    }
    .cover {
      width: 56px;
      height: 56px;
    }
    .actions {
      grid-column: 1 / -1;
      flex-direction: row;
      flex-wrap: wrap;
      min-width: 0;
    }
    .actions button {
      flex: 1 1 auto;
      min-width: calc(50% - 0.25rem);
    }
    .reject-field {
      flex: 1 1 100%;
    }
  }
</style>
