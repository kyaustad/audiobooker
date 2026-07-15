<script lang="ts">
  import { onDestroy, onMount } from 'svelte'
  import { push } from 'svelte-spa-router'
  import { api, type ContentEntry, type Download, type Library } from '../lib/api'
  import { showToast } from '../lib/toast'

  let { params = { id: '' } }: { params?: { id: string } } = $props()

  let download = $state<Download | null>(null)
  let libraries = $state<Library[]>([])
  let files = $state<ContentEntry[]>([])
  let filesSource = $state('none')
  let loading = $state(true)
  let selectedPath = $state<string | null>(null)
  let title = $state('')
  let author = $state('')
  let asin = $state('')
  let matches = $state<any[]>([])
  let searching = $state(false)
  let saving = $state(false)
  let libraryId = $state<number | null>(null)
  let timer: number | undefined

  const mappedPaths = $derived(new Set((download?.items || []).map((i) => i.source_path)))

  const topDirs = $derived(
    files.filter((f) => f.is_dir && !f.path.includes('/')),
  )
  const showFiles = $derived(
    topDirs.length ? topDirs : files.filter((f) => !f.path.includes('/') || f.is_dir),
  )

  function parseFolderName(path: string) {
    const name = path.split('/').filter(Boolean).pop() || path
    const idx = name.lastIndexOf(' - ')
    if (idx > 0) {
      return { title: name.slice(0, idx).trim(), author: name.slice(idx + 3).trim() }
    }
    return { title: name, author: '' }
  }

  async function refresh() {
    const id = Number(params.id)
    const [data, libs, fileData] = await Promise.all([
      api.getDownload(id),
      api.myLibraries(),
      api.downloadFiles(id).catch(() => ({ files: [] as ContentEntry[], source: 'none' })),
    ])
    download = data.download
    libraries = libs.libraries
    files = fileData.files
    filesSource = fileData.source
    if (libraries.length === 1) libraryId = libraries[0].id
    if (download.kind !== 'pack') {
      push(`/match/${id}`)
    }
  }

  onMount(() => {
    refresh()
      .catch((e) => showToast(e.message))
      .finally(() => (loading = false))
    timer = window.setInterval(() => {
      refresh().catch(() => undefined)
    }, 10000)
  })

  onDestroy(() => {
    if (timer) clearInterval(timer)
  })

  function selectSource(path: string) {
    selectedPath = path
    const parsed = parseFolderName(path)
    title = parsed.title
    author = parsed.author
    asin = ''
    matches = []
  }

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

  async function mapMatch(m: any) {
    if (!selectedPath) {
      showToast('Select a folder or file first')
      return
    }
    if (libraries.length > 1 && !libraryId) {
      showToast('Select which library to import into')
      return
    }
    saving = true
    try {
      const data = await api.mapDownloadItem(Number(params.id), {
        source_path: selectedPath,
        match_data: m,
        library_id: libraryId ?? undefined,
      })
      download = data.download
      showToast(`Mapped ${m.title}`)
      selectedPath = null
      matches = []
    } catch (err) {
      showToast(err instanceof Error ? err.message : 'Could not map')
    } finally {
      saving = false
    }
  }

  async function unmap(itemId: number) {
    try {
      const data = await api.unmapDownloadItem(Number(params.id), itemId)
      download = data.download
      showToast('Unmapped')
    } catch (err) {
      showToast(err instanceof Error ? err.message : 'Could not unmap')
    }
  }

  function formatSize(n: number) {
    if (!n) return ''
    const u = ['B', 'KB', 'MB', 'GB']
    const i = Math.min(Math.floor(Math.log(n) / Math.log(1024)), u.length - 1)
    return `${(n / 1024 ** i).toFixed(1)} ${u[i]}`
  }
</script>

{#if loading || !download}
  <div class="card muted">Loading pack map…</div>
{:else}
  <div class="card stack">
    <div class="row" style="justify-content:space-between;align-items:start">
      <div>
        <h2>Map pack books</h2>
        <p class="muted">
          {download.name || 'Pack torrent'} · <span class="badge {download.status}">{download.status.replaceAll('_', ' ')}</span>
          · files from {filesSource}
        </p>
        <p class="muted">
          Select a folder (or file), match it on Audible, and save. Imports run automatically after the torrent finishes for each mapped item.
        </p>
      </div>
      <a class="btn secondary" href="#/">Back to queue</a>
    </div>
  </div>

  <div class="map-layout">
    <div class="card stack">
      <h3>Sources</h3>
      {#if !files.length}
        <p class="muted">No files yet — wait until qBittorrent has metadata or the download completes, then refresh.</p>
        <button class="secondary" type="button" onclick={() => refresh()}>Refresh files</button>
      {:else}
        <div class="file-list">
          {#each showFiles as f}
            {@const mapped = mappedPaths.has(f.path)}
            <button
              type="button"
              class="file-row"
              class:selected={selectedPath === f.path}
              class:mapped
              disabled={mapped}
              onclick={() => selectSource(f.path)}
            >
              <span class="file-kind">{f.is_dir ? 'DIR' : 'FILE'}</span>
              <span class="file-name">{f.path}</span>
              {#if mapped}<em>mapped</em>{/if}
              {#if !f.is_dir && f.size}<span class="muted">{formatSize(f.size)}</span>{/if}
            </button>
          {/each}
        </div>
        {#if files.some((f) => f.path.includes('/'))}
          <details>
            <summary class="muted">All paths ({files.length})</summary>
            <div class="file-list">
              {#each files as f}
                {@const mapped = mappedPaths.has(f.path)}
                <button
                  type="button"
                  class="file-row"
                  class:selected={selectedPath === f.path}
                  class:mapped
                  disabled={mapped}
                  onclick={() => selectSource(f.path)}
                >
                  <span class="file-kind">{f.is_dir ? 'DIR' : 'FILE'}</span>
                  <span class="file-name">{f.path}</span>
                  {#if mapped}<em>mapped</em>{/if}
                </button>
              {/each}
            </div>
          </details>
        {/if}
      {/if}

      <h3>Mapped ({download.items?.length || 0})</h3>
      {#if !download.items?.length}
        <p class="muted">Nothing mapped yet.</p>
      {:else}
        {#each download.items as item}
          <div class="mapped-row">
            <div>
              <strong>{item.metadata?.title || item.source_path}</strong>
              <div class="muted">{item.source_path}</div>
              <span class="badge {item.status}">{item.status}</span>
              {#if item.error_message}
                <div style="color:var(--danger);font-size:0.85rem">{item.error_message}</div>
              {/if}
            </div>
            {#if item.status !== 'imported' && item.status !== 'copying'}
              <button class="danger" type="button" onclick={() => unmap(item.id)}>Unmap</button>
            {/if}
          </div>
        {/each}
      {/if}
    </div>

    <div class="card stack">
      <h3>Audible match{#if selectedPath} for <code>{selectedPath}</code>{/if}</h3>
      {#if !selectedPath}
        <p class="muted">Select a source path on the left.</p>
      {:else}
        {#if libraries.length > 1}
          <label>Library
            <select
              value={libraryId ?? ''}
              onchange={(e) => {
                const v = (e.currentTarget as HTMLSelectElement).value
                libraryId = v ? Number(v) : null
              }}
            >
              <option value="" disabled>Select library…</option>
              {#each libraries as lib}
                <option value={lib.id}>{lib.name}</option>
              {/each}
            </select>
          </label>
        {:else if libraries[0]}
          <p class="muted">Library: {libraries[0].name}</p>
        {/if}

        <form class="stack" onsubmit={search}>
          <div class="row">
            <label>Title
              <input bind:value={title} />
            </label>
            <label>Author
              <input bind:value={author} />
            </label>
          </div>
          <label>Or ASIN
            <input bind:value={asin} />
          </label>
          <button type="submit" disabled={searching}>{searching ? 'Searching…' : 'Search Audible'}</button>
        </form>

        {#if matches.length}
          <div class="match-grid compact">
            {#each matches as m}
              <button class="match-card" type="button" disabled={saving} onclick={() => mapMatch(m)}>
                <img src={m.cover_url || '/icons/icon-192.png'} alt="" />
                <strong class="match-title">{m.title}</strong>
                <div class="muted">{(m.authors || []).join(', ')}</div>
              </button>
            {/each}
          </div>
        {/if}
      {/if}
    </div>
  </div>
{/if}

<style>
  .map-layout {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 1rem;
    align-items: start;
  }
  .file-list {
    display: grid;
    gap: 0.35rem;
    max-height: 28rem;
    overflow: auto;
  }
  .file-row {
    display: grid;
    grid-template-columns: auto 1fr auto;
    gap: 0.45rem;
    align-items: center;
    text-align: left;
    background: var(--bg) !important;
    color: var(--text) !important;
    border: 1px solid var(--border) !important;
    border-radius: 8px;
    padding: 0.45rem 0.55rem !important;
    font-weight: 400;
  }
  .file-row.selected {
    border-color: var(--accent) !important;
  }
  .file-row.mapped {
    opacity: 0.55;
  }
  .file-kind {
    font-size: 0.7rem;
    font-weight: 700;
    color: var(--muted);
  }
  .file-name {
    font-size: 0.88rem;
    word-break: break-all;
  }
  .mapped-row {
    display: flex;
    justify-content: space-between;
    gap: 0.75rem;
    align-items: start;
    border-bottom: 1px solid var(--border);
    padding-bottom: 0.55rem;
  }
  .match-grid.compact {
    grid-template-columns: repeat(auto-fill, minmax(140px, 1fr));
  }
  code {
    font-family: var(--mono);
    font-size: 0.85em;
  }
  @media (max-width: 900px) {
    .map-layout { grid-template-columns: 1fr; }
  }
</style>
