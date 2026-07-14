<script lang="ts">
  import { push } from 'svelte-spa-router'
  import { api } from '../lib/api'
  import { showToast } from '../lib/toast'

  let q = $state('')
  let page = $state(1)
  let hasMore = $state(false)
  let results = $state<any[]>([])
  let loading = $state(false)
  let adding = $state<string | null>(null)
  let hasSearched = $state(false)
  let error = $state<string | null>(null)

  async function search(e?: Event, nextPage = 1) {
    e?.preventDefault()
    if (!q.trim()) return
    loading = true
    error = null
    hasSearched = true
    try {
      const data = await api.abbSearch(q, nextPage)
      results = data.results
      page = data.page ?? nextPage
      hasMore = Boolean(data.has_more)
      if (!results.length) {
        error =
          nextPage > 1
            ? 'No more results on this page.'
            : 'No results found. Try different keywords — mirrors or site layout can vary.'
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

  async function add(item: any) {
    adding = item.url
    try {
      const { details } = await api.abbDetails(item.url)
      const input = details.magnet_uri || details.info_hash
      if (!input) throw new Error('Could not find info hash on that page')
      const { download } = await api.createDownload(input, details.title || item.title)
      showToast('Added — match Audible metadata next')
      push(`/match/${download.id}`)
    } catch (err) {
      showToast(err instanceof Error ? err.message : 'Add failed')
    } finally {
      adding = null
    }
  }
</script>

<div class="abb-shell">
  <header class="abb-hero card">
    <div class="row" style="justify-content:space-between;align-items:flex-start">
      <div>
        <p class="abb-kicker">Secondary browser</p>
        <h2 style="margin:0">AudiobookBay</h2>
        <p class="muted" style="margin-top:0.4rem">
          Search mirrors (audiobookbay.lu, etc.), then Add &amp; match to Audible before downloading.
          This is separate from your main queue — hashes are not sent to qBittorrent until you match.
        </p>
      </div>
      <a class="btn secondary" href="#/">Back to queue</a>
    </div>

    <form class="abb-search" onsubmit={(e) => search(e, 1)}>
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
    <div class="card" style="border-color:color-mix(in oklab, var(--warning) 45%, var(--border))">
      <p style="margin:0;color:var(--warning)">{error}</p>
    </div>
  {/if}

  {#if loading}
    <div class="card muted">Searching mirrors…</div>
  {:else if hasSearched && results.length}
    <div class="abb-meta muted">Page {page} · {results.length} results</div>
    <div class="abb-grid">
      {#each results as r}
        <article class="abb-card">
          <img src={r.cover_url || '/favicon.svg'} alt="" loading="lazy" />
          <div class="abb-card-body">
            <h3>{r.title}</h3>
            {#if r.info}<p class="muted">{r.info}</p>{/if}
            <div class="row">
              <button type="button" disabled={adding === r.url} onclick={() => add(r)}>
                {adding === r.url ? 'Adding…' : 'Add & match'}
              </button>
              <a class="btn secondary" href={r.url} target="_blank" rel="noreferrer">Open source</a>
            </div>
          </div>
        </article>
      {/each}
    </div>
    <div class="row" style="justify-content:center;margin:1rem 0 2rem">
      <button class="secondary" type="button" disabled={page <= 1 || loading} onclick={() => search(undefined, page - 1)}>
        Previous
      </button>
      <button class="secondary" type="button" disabled={loading || !hasMore} onclick={() => search(undefined, page + 1)}>
        Next page
      </button>
    </div>
  {:else if hasSearched}
    <div class="card muted">No listings to show.</div>
  {:else}
    <div class="card muted">
      Use search to browse AudiobookBay from inside Audiobooker. Treat this as a convenience UI —
      respect the site’s terms and availability.
    </div>
  {/if}
</div>

<style>
  .abb-shell {
    margin: -0.25rem -0.25rem 0;
  }
  .abb-hero {
    background:
      linear-gradient(135deg, rgba(91, 159, 212, 0.12), transparent 50%),
      var(--bg-elevated);
  }
  .abb-kicker {
    margin: 0 0 0.25rem;
    font-size: 0.75rem;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--accent);
    font-weight: 700;
  }
  .abb-search {
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 0.65rem;
    margin-top: 1rem;
  }
  .abb-search input {
    font-size: 1.05rem;
    padding: 0.75rem 0.9rem;
  }
  .abb-meta {
    margin: 0.25rem 0 0.75rem;
    font-size: 0.85rem;
  }
  .abb-grid {
    display: grid;
    gap: 0.85rem;
  }
  .abb-card {
    display: grid;
    grid-template-columns: 88px 1fr;
    gap: 0.9rem;
    background: var(--bg-elevated);
    border: 1px solid var(--border);
    border-radius: var(--radius);
    padding: 0.85rem;
  }
  .abb-card img {
    width: 88px;
    height: 88px;
    object-fit: cover;
    border-radius: 8px;
    background: var(--bg);
    border: 1px solid var(--border);
  }
  .abb-card-body h3 {
    margin: 0 0 0.35rem;
    font-size: 1rem;
    line-height: 1.3;
  }
  .abb-card-body p {
    margin: 0 0 0.7rem;
    font-size: 0.85rem;
  }
  @media (max-width: 640px) {
    .abb-search { grid-template-columns: 1fr; }
    .abb-card { grid-template-columns: 64px 1fr; }
    .abb-card img { width: 64px; height: 64px; }
  }
</style>
