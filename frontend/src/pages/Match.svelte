<script lang="ts">
  import { push } from 'svelte-spa-router'
  import { api } from '../lib/api'
  import { showToast } from '../lib/toast'

  let { params = { id: '' } }: { params?: { id: string } } = $props()

  let title = $state('')
  let author = $state('')
  let asin = $state('')
  let matches = $state<any[]>([])
  let searching = $state(false)
  let saving = $state(false)

  async function search(e?: Event) {
    e?.preventDefault()
    searching = true
    try {
      if (asin.trim()) {
        const data = await api.metadataByAsin(asin.trim())
        matches = [data.match]
      } else {
        const data = await api.searchMetadata(title, author || undefined)
        matches = data.matches
      }
      if (!matches.length) showToast('No matches found')
    } catch (err) {
      showToast(err instanceof Error ? err.message : 'Search failed')
    } finally {
      searching = false
    }
  }

  async function choose(m: any) {
    saving = true
    try {
      await api.matchDownload(Number(params.id), m)
      showToast('Matched and sent to qBittorrent')
      push('/')
    } catch (err) {
      showToast(err instanceof Error ? err.message : 'Match failed')
    } finally {
      saving = false
    }
  }
</script>

<div class="card stack">
    <div>
      <h2>Match Audible metadata</h2>
      <p class="muted">
        Search uses Audible’s catalog (same approach as Audiobookshelf), then enriches via Audnexus.
        Paste an ASIN for a direct match.
      </p>
    </div>
  <form class="stack" onsubmit={search}>
    <div class="row">
      <label>Title
        <input bind:value={title} placeholder="Book title" />
      </label>
      <label>Author
        <input bind:value={author} placeholder="Optional" />
      </label>
    </div>
    <label>Or ASIN
      <input bind:value={asin} placeholder="B0XXXXXXXX" />
    </label>
    <button type="submit" disabled={searching}>{searching ? 'Searching…' : 'Search Audible'}</button>
  </form>
</div>

{#if matches.length}
  <div class="match-grid">
    {#each matches as m}
      <button class="match-card" type="button" disabled={saving} onclick={() => choose(m)}>
        <img src={m.cover_url || '/favicon.svg'} alt="" />
        <strong class="match-title">{m.title}</strong>
        {#if m.subtitle}
          <div class="match-subtitle">{m.subtitle}</div>
        {/if}
        <div class="muted">{(m.authors || []).join(', ')}</div>
        {#if m.series}
          <div class="muted">{m.series}{m.series_index ? ` #${m.series_index}` : ''}</div>
        {/if}
        <div class="muted">ASIN {m.asin}</div>
      </button>
    {/each}
  </div>
{/if}
