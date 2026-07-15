<script lang="ts">
  import { onDestroy, onMount } from 'svelte'
  import { push } from 'svelte-spa-router'
  import { api, type AbbDetails, type AbbSearchResult, type Download } from '../lib/api'
  import { showToast } from '../lib/toast'

  let q = $state('')
  let page = $state(1)
  let hasMore = $state(false)
  let mirror = $state<string | null>(null)
  let results = $state<AbbSearchResult[]>([])
  let loading = $state(false)
  let loadingMore = $state(false)
  let requesting = $state(false)
  let hasSearched = $state(false)
  let error = $state<string | null>(null)
  let selected = $state<AbbSearchResult | null>(null)
  let details = $state<AbbDetails | null>(null)
  let detailsLoading = $state(false)
  let queue = $state<Download[]>([])
  let sentinelEl = $state<HTMLElement | null>(null)
  let observer: IntersectionObserver | undefined

  async function refreshQueue() {
    try {
      const data = await api.listDownloads()
      queue = data.downloads
    } catch {
      /* ignore */
    }
  }

  function queueHit(item: AbbSearchResult) {
    const needle = (item.title || '').toLowerCase()
    return queue.find((d) => {
      const name = (d.name || d.metadata?.title || '').toLowerCase()
      return name && needle && (name.includes(needle) || needle.includes(name))
    })
  }

  async function search(e?: Event) {
    e?.preventDefault()
    if (!q.trim()) return
    loading = true
    loadingMore = false
    error = null
    hasSearched = true
    selected = null
    details = null
    page = 1
    results = []
    try {
      const [data] = await Promise.all([api.abbSearch(q, 1), refreshQueue()])
      results = data.results
      page = data.page ?? 1
      hasMore = Boolean(data.has_more)
      mirror = data.mirror ?? null
      if (!results.length) {
        error = 'No results found. Try a simpler title, or author + title.'
      }
    } catch (err) {
      results = []
      hasMore = false
      error = err instanceof Error ? err.message : 'Search failed'
      showToast(error)
    } finally {
      loading = false
    }
  }

  async function loadMore() {
    if (!hasMore || loading || loadingMore || !q.trim()) return
    loadingMore = true
    try {
      const next = page + 1
      const data = await api.abbSearch(q, next)
      const seen = new Set(results.map((r) => r.url))
      results = [...results, ...data.results.filter((r) => !seen.has(r.url))]
      page = data.page ?? next
      hasMore = Boolean(data.has_more)
      mirror = data.mirror ?? mirror
    } catch (err) {
      showToast(err instanceof Error ? err.message : 'Could not load more')
      hasMore = false
    } finally {
      loadingMore = false
    }
  }

  onMount(() => {
    observer = new IntersectionObserver(
      (entries) => {
        if (entries.some((entry) => entry.isIntersecting)) loadMore()
      },
      { rootMargin: '400px' },
    )
  })

  $effect(() => {
    const el = sentinelEl
    if (!observer || !el) return
    observer.observe(el)
    return () => observer?.unobserve(el)
  })

  onDestroy(() => observer?.disconnect())

  async function openDetails(item: AbbSearchResult) {
    selected = item
    details = null
    detailsLoading = true
    try {
      const data = await api.abbDetails(item.url)
      details = data.details
    } catch (err) {
      showToast(err instanceof Error ? err.message : 'Could not load details')
    } finally {
      detailsLoading = false
    }
  }

  function closeDetails() {
    selected = null
    details = null
  }

  async function requestSelected() {
    if (!selected) return
    requesting = true
    try {
      let detail = details
      if (!detail) {
        const data = await api.abbDetails(selected.url)
        detail = data.details
        details = detail
      }
      const input = detail.magnet_uri || detail.info_hash
      if (!input) throw new Error('Could not find info hash on that page')

      const displayName = [detail.title || selected.title, detail.author || selected.author]
        .filter(Boolean)
        .join(' - ')

      const { download } = await api.createDownload(input, displayName)
      showToast('Requested — match Audible metadata next')
      closeDetails()
      push(`/match/${download.id}`)
    } catch (err) {
      showToast(err instanceof Error ? err.message : 'Request failed')
    } finally {
      requesting = false
    }
  }
</script>

<div class="discover">
  <header class="discover-hero">
    <div class="discover-hero-copy">
      <p class="abb-kicker">Discover</p>
      <h1>AudiobookBay</h1>
      <p class="muted">
        Same query params as
        <a href="https://audiobookbay.lu/?s=sunrise+on+the+reaping&cat=undefined%2Cundefined" target="_blank" rel="noreferrer">audiobookbay.lu</a>
        — open a title, then request it into your queue.
      </p>
    </div>
    <form class="discover-search" onsubmit={search}>
      <input
        bind:value={q}
        required
        placeholder="Search title, author, series…"
        aria-label="Search AudiobookBay"
      />
      <button type="submit" disabled={loading}>{loading ? 'Searching…' : 'Search'}</button>
    </form>
  </header>

  {#if error}
    <div class="banner-warn">{error}</div>
  {/if}

  {#if loading && !results.length}
    <div class="poster-grid">
      {#each Array(8) as _}
        <div class="poster skeleton"></div>
      {/each}
    </div>
  {:else if hasSearched && results.length}
    <div class="discover-meta muted">
      {results.length} results{#if hasMore}+{/if}
      {#if mirror}<span>· via {mirror.replace(/^https?:\/\//, '')}</span>{/if}
    </div>
    <div class="poster-grid">
      {#each results as r}
        {@const hit = queueHit(r)}
        <button type="button" class="poster" onclick={() => openDetails(r)}>
          <div class="poster-art">
            <img src={r.cover_url || '/icons/icon-192.png'} alt="" loading="lazy" />
            {#if hit}
              <span class="poster-status">{hit.status === 'imported' ? 'In library' : 'Requested'}</span>
            {/if}
          </div>
          <div class="poster-body">
            <strong>{r.title}</strong>
            {#if r.author}<span class="muted">{r.author}</span>{/if}
            <span class="poster-chips">
              {#if r.format}<em>{r.format}</em>{/if}
              {#if r.size}<em>{r.size}</em>{/if}
            </span>
          </div>
        </button>
      {/each}
    </div>
    <div class="scroll-sentinel" bind:this={sentinelEl} aria-hidden="true"></div>
    {#if loadingMore}
      <p class="muted" style="text-align:center">Loading more…</p>
    {:else if !hasMore}
      <p class="muted" style="text-align:center;margin:1rem 0 1.5rem">End of results</p>
    {/if}
  {:else if hasSearched}
    <div class="empty muted">No listings to show.</div>
  {:else}
    <div class="empty muted">
      Start with a search — results stay in AudiobookBay order. Requesting adds the torrent to your queue for Audible matching.
    </div>
  {/if}
</div>

{#if selected}
  <div
    class="drawer-backdrop"
    role="presentation"
    onclick={closeDetails}
    onkeydown={(e) => e.key === 'Escape' && closeDetails()}
  >
    <div
      class="drawer"
      role="dialog"
      aria-modal="true"
      aria-label={details?.title || selected.title}
      tabindex="-1"
      onclick={(e) => e.stopPropagation()}
      onkeydown={(e) => e.stopPropagation()}
    >
      <button class="drawer-close secondary" type="button" onclick={closeDetails}>Close</button>
      <div class="drawer-art">
        <img
          src={details?.cover_url || selected.cover_url || '/icons/icon-192.png'}
          alt=""
        />
      </div>
      <h2>{details?.title || selected.title}</h2>
      {#if details?.author || selected.author}
        <p class="muted author">{details?.author || selected.author}</p>
      {/if}
      {#if details?.narrator}
        <p class="muted">Narrated by {details.narrator}</p>
      {/if}

      <div class="drawer-chips">
        {#if details?.format || selected.format}<span>{details?.format || selected.format}</span>{/if}
        {#if details?.bitrate || selected.bitrate}<span>{details?.bitrate || selected.bitrate}</span>{/if}
        {#if details?.size || selected.size}<span>{details?.size || selected.size}</span>{/if}
        {#if selected.language}<span>{selected.language}</span>{/if}
        {#if selected.posted}<span>Posted {selected.posted}</span>{/if}
      </div>

      {#if detailsLoading}
        <p class="muted">Loading details…</p>
      {:else if details?.description}
        <p class="drawer-desc">{details.description}</p>
      {:else if selected.info}
        <p class="drawer-desc muted">{selected.info}</p>
      {/if}

      <div class="drawer-actions">
        <button type="button" disabled={requesting || detailsLoading} onclick={requestSelected}>
          {requesting ? 'Requesting…' : 'Request'}
        </button>
        <a class="btn secondary" href={selected.url} target="_blank" rel="noreferrer">Open on ABB</a>
      </div>
      <p class="muted tiny">
        Request adds the magnet to your queue. You’ll match Audible metadata before qBittorrent starts.
      </p>
    </div>
  </div>
{/if}

<style>
  .discover {
    margin: -0.25rem 0 0;
  }
  .discover-hero {
    padding: 1.25rem 1.2rem 1.35rem;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background:
      linear-gradient(125deg, rgba(91, 159, 212, 0.16), transparent 55%),
      linear-gradient(to bottom, color-mix(in oklab, var(--bg-elevated) 92%, black), var(--bg));
    margin-bottom: 1rem;
  }
  .discover-hero h1 {
    margin: 0;
    font-size: clamp(1.75rem, 4vw, 2.35rem);
    letter-spacing: -0.03em;
    line-height: 1.1;
  }
  .discover-hero-copy .muted {
    margin: 0.45rem 0 0;
    max-width: 42rem;
  }
  .abb-kicker {
    margin: 0 0 0.3rem;
    font-size: 0.75rem;
    letter-spacing: 0.1em;
    text-transform: uppercase;
    color: var(--accent);
    font-weight: 700;
  }
  .discover-search {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 0.65rem;
    margin-top: 1.1rem;
  }
  .discover-search input {
    font-size: 1.05rem;
    padding: 0.8rem 0.95rem;
  }
  .discover-meta {
    margin: 0 0 0.85rem;
    font-size: 0.85rem;
  }
  .poster-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
    gap: 0.9rem;
  }
  .poster {
    display: grid;
    gap: 0.55rem;
    text-align: left;
    background: transparent !important;
    color: var(--text) !important;
    border: none !important;
    padding: 0 !important;
    font-weight: 400;
    cursor: pointer;
    border-radius: 0;
  }
  .poster:hover {
    background: transparent !important;
    color: var(--text) !important;
  }
  .poster:hover .poster-art {
    border-color: var(--accent);
    transform: translateY(-2px);
  }
  .poster-art {
    position: relative;
    aspect-ratio: 1;
    border-radius: 10px;
    overflow: hidden;
    border: 1px solid var(--border);
    background: var(--bg-elevated);
    transition: border-color 0.15s ease, transform 0.15s ease;
  }
  .poster-art img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }
  .poster-status {
    position: absolute;
    left: 0.45rem;
    top: 0.45rem;
    background: color-mix(in oklab, var(--bg) 75%, black);
    border: 1px solid var(--border);
    color: var(--accent);
    font-size: 0.7rem;
    font-weight: 700;
    padding: 0.15rem 0.4rem;
    border-radius: 6px;
  }
  .poster-body {
    display: grid;
    gap: 0.15rem;
  }
  .poster-body strong {
    font-size: 0.92rem;
    line-height: 1.25;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
  }
  .poster-chips {
    display: flex;
    flex-wrap: wrap;
    gap: 0.3rem;
  }
  .poster-chips em {
    font-style: normal;
    font-size: 0.72rem;
    color: var(--muted);
    border: 1px solid var(--border);
    border-radius: 999px;
    padding: 0.05rem 0.4rem;
  }
  .skeleton {
    aspect-ratio: 0.72;
    border-radius: 10px;
    background: linear-gradient(90deg, var(--bg-elevated), var(--bg-hover), var(--bg-elevated));
    background-size: 200% 100%;
    animation: shimmer 1.2s ease-in-out infinite;
    border: 1px solid var(--border);
  }
  @keyframes shimmer {
    0% { background-position: 100% 0; }
    100% { background-position: -100% 0; }
  }
  .scroll-sentinel {
    height: 1px;
    width: 100%;
  }
  .empty, .banner-warn {
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 1rem;
    background: var(--bg-elevated);
    margin-bottom: 1rem;
  }
  .banner-warn {
    border-color: color-mix(in oklab, var(--warning) 45%, var(--border));
    color: var(--warning);
  }
  .drawer-backdrop {
    position: fixed;
    inset: 0;
    background: rgba(4, 8, 12, 0.62);
    backdrop-filter: blur(4px);
    z-index: 40;
    display: flex;
    justify-content: flex-end;
  }
  .drawer {
    width: min(420px, 100%);
    height: 100%;
    overflow: auto;
    background: var(--bg-elevated);
    border-left: 1px solid var(--border);
    padding: 1rem 1.1rem 1.5rem;
    animation: slide-in 0.2s ease;
  }
  @keyframes slide-in {
    from { transform: translateX(12px); opacity: 0.6; }
    to { transform: translateX(0); opacity: 1; }
  }
  .drawer-close {
    float: right;
    margin-bottom: 0.5rem;
  }
  .drawer-art {
    clear: both;
    aspect-ratio: 1;
    max-width: 220px;
    margin: 0.25rem auto 1rem;
    border-radius: 12px;
    overflow: hidden;
    border: 1px solid var(--border);
    background: var(--bg);
  }
  .drawer-art img {
    width: 100%;
    height: 100%;
    object-fit: cover;
    display: block;
  }
  .drawer h2 {
    margin: 0 0 0.25rem;
    font-size: 1.25rem;
    line-height: 1.25;
  }
  .drawer .author { margin: 0 0 0.35rem; }
  .drawer-chips {
    display: flex;
    flex-wrap: wrap;
    gap: 0.35rem;
    margin: 0.75rem 0;
  }
  .drawer-chips span {
    font-size: 0.78rem;
    border: 1px solid var(--border);
    border-radius: 999px;
    padding: 0.15rem 0.55rem;
    color: var(--muted);
  }
  .drawer-desc {
    margin: 0 0 1rem;
    font-size: 0.9rem;
    line-height: 1.45;
    max-height: 10rem;
    overflow: auto;
  }
  .drawer-actions {
    display: flex;
    gap: 0.6rem;
    flex-wrap: wrap;
    margin-bottom: 0.75rem;
  }
  .tiny { font-size: 0.8rem; }
  @media (max-width: 640px) {
    .discover-search { grid-template-columns: 1fr; }
    .poster-grid { grid-template-columns: repeat(auto-fill, minmax(132px, 1fr)); }
    .drawer { width: 100%; }
  }
</style>
