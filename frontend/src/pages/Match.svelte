<script lang="ts">
  import { onMount } from 'svelte'
  import { push } from 'svelte-spa-router'
  import { api, type Library } from '../lib/api'
  import { showToast } from '../lib/toast'

  let { params = { id: '' } }: { params?: { id: string } } = $props()

  let title = $state('')
  let author = $state('')
  let asin = $state('')
  let matches = $state<any[]>([])
  let searching = $state(false)
  let saving = $state(false)
  let loading = $state(true)
  let displayName = $state<string | null>(null)
  let libraries = $state<Library[]>([])
  let libraryId = $state<number | null>(null)

  function parseName(name: string | null | undefined) {
    if (!name) return { title: '', author: '' }
    const cleaned = name.trim()
    const idx = cleaned.lastIndexOf(' - ')
    if (idx > 0) {
      return {
        title: cleaned.slice(0, idx).trim(),
        author: cleaned.slice(idx + 3).trim(),
      }
    }
    return { title: cleaned, author: '' }
  }

  onMount(async () => {
    try {
      const [data, libs] = await Promise.all([
        api.getDownload(Number(params.id)),
        api.myLibraries(),
      ])
      libraries = libs.libraries
      if (libraries.length === 1) libraryId = libraries[0].id
      else if (data.download.library_id) libraryId = data.download.library_id

      displayName = data.download.name
      const parsed = parseName(data.download.name)
      title = parsed.title
      author = parsed.author
      if (title) await search()
    } catch (err) {
      showToast(err instanceof Error ? err.message : 'Could not load download')
    } finally {
      loading = false
    }
  })

  async function search(e?: Event) {
    e?.preventDefault()
    searching = true
    try {
      if (asin.trim()) {
        const data = await api.metadataByAsin(asin.trim())
        matches = [data.match]
      } else {
        if (!title.trim()) {
          showToast('Enter a title or ASIN')
          return
        }
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
    if (libraries.length > 1 && !libraryId) {
      showToast('Select which library to import into')
      return
    }
    saving = true
    try {
      await api.matchDownload(
        Number(params.id),
        m,
        libraryId ?? undefined,
      )
      showToast('Matched and sent to qBittorrent')
      push('/')
    } catch (err) {
      showToast(err instanceof Error ? err.message : 'Match failed')
    } finally {
      saving = false
    }
  }
</script>

{#if loading}
  <div class="card muted">Loading match…</div>
{:else}
  <div class="card stack">
    <div>
      <h2>Match Audible metadata</h2>
      <p class="muted">
        Search uses Audible’s catalog, then enriches via Audnexus.
        {#if displayName}
          Prefilling from <strong>{displayName}</strong>.
        {/if}
      </p>
    </div>

    {#if libraries.length === 0}
      <div class="banner-warn">No libraries assigned to your account. Ask an admin to grant access.</div>
    {:else if libraries.length > 1}
      <label>Import into library
        <select
          value={libraryId ?? ''}
          onchange={(e) => {
            const v = (e.currentTarget as HTMLSelectElement).value
            libraryId = v ? Number(v) : null
          }}
          required
        >
          <option value="" disabled>Select library…</option>
          {#each libraries as lib}
            <option value={lib.id}>{lib.name} ({lib.path})</option>
          {/each}
        </select>
      </label>
    {:else}
      <p class="muted">Library: <strong>{libraries[0].name}</strong> <span class="muted">({libraries[0].path})</span></p>
    {/if}

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

  {#if searching && !matches.length}
    <div class="card muted">Searching Audible…</div>
  {/if}

  {#if matches.length}
    <div class="match-grid">
      {#each matches as m}
        <button class="match-card" type="button" disabled={saving || libraries.length === 0} onclick={() => choose(m)}>
          <img src={m.cover_url || '/icons/icon-192.png'} alt="" />
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
{/if}

<style>
  .banner-warn {
    border: 1px solid color-mix(in oklab, var(--warning) 45%, var(--border));
    color: var(--warning);
    border-radius: var(--radius);
    padding: 0.75rem 0.9rem;
    background: var(--bg);
  }
</style>
