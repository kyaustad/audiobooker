<script lang="ts">
  import { onDestroy, onMount } from 'svelte'
  import { push } from 'svelte-spa-router'
  import { api, type ContentEntry, type Download, type Library } from '../lib/api'
  import { showToast } from '../lib/toast'

  let { params = { id: '' } }: { params?: { id: string } } = $props()

  type TreeNode = {
    name: string
    path: string
    is_dir: boolean
    size: number
    children: TreeNode[]
  }

  let download = $state<Download | null>(null)
  let libraries = $state<Library[]>([])
  let files = $state<ContentEntry[]>([])
  let filesSource = $state('none')
  let contentPath = $state<string | null>(null)
  let loading = $state(true)
  let selectedPath = $state<string | null>(null)
  let selectedPaths = $state<Set<string>>(new Set())
  let selectedIsDir = $state(false)
  let title = $state('')
  let author = $state('')
  let asin = $state('')
  let matches = $state<any[]>([])
  let searching = $state(false)
  let saving = $state(false)
  let retrying = $state(false)
  let refreshingQbit = $state(false)
  let libraryId = $state<number | null>(null)
  let expanded = $state<Set<string>>(new Set())
  let timer: number | undefined

  const mappedPaths = $derived(
    new Set(
      (download?.items || []).flatMap((i) =>
        i.source_paths?.length ? i.source_paths : [i.source_path],
      ),
    ),
  )
  const failedCount = $derived(
    (download?.items || []).filter((i) => i.status === 'error').length,
  )
  const copyingCount = $derived(
    (download?.items || []).filter((i) => i.status === 'copying').length,
  )
  const retryableCount = $derived(failedCount + copyingCount)
  const tree = $derived(buildTree(files))
  const selectionCount = $derived(selectedPaths.size)

  function stripExtension(name: string) {
    return name.replace(/\.(m4b|m4a|mp3|flac|ogg|opus|aac|wma|wav|mp4|mka|pdf|epub)$/i, '')
  }

  function parseFolderName(path: string) {
    const raw = path.split('/').filter(Boolean).pop() || path
    return { title: stripExtension(raw), author: '' }
  }

  function buildTree(entries: ContentEntry[]): TreeNode[] {
    type Mutable = TreeNode & { map: Map<string, Mutable> }
    const root: Mutable = {
      name: '',
      path: '',
      is_dir: true,
      size: 0,
      children: [],
      map: new Map(),
    }

    const ensureDir = (parent: Mutable, name: string, path: string) => {
      let node = parent.map.get(name)
      if (!node) {
        node = { name, path, is_dir: true, size: 0, children: [], map: new Map() }
        parent.map.set(name, node)
        parent.children.push(node)
      }
      return node
    }

    for (const entry of entries) {
      const parts = entry.path.split('/').filter(Boolean)
      if (!parts.length) continue
      let cursor = root
      let acc = ''
      for (let i = 0; i < parts.length; i++) {
        const part = parts[i]
        acc = acc ? `${acc}/${part}` : part
        const isLast = i === parts.length - 1
        if (isLast && !entry.is_dir) {
          if (!cursor.map.has(part)) {
            const fileNode: Mutable = {
              name: part,
              path: entry.path,
              is_dir: false,
              size: entry.size,
              children: [],
              map: new Map(),
            }
            cursor.map.set(part, fileNode)
            cursor.children.push(fileNode)
          }
        } else {
          cursor = ensureDir(cursor, part, acc)
          if (isLast && entry.is_dir) {
            cursor.is_dir = true
          }
        }
      }
    }

    const sortNodes = (nodes: TreeNode[]) => {
      nodes.sort((a, b) => {
        if (a.is_dir !== b.is_dir) return a.is_dir ? -1 : 1
        return a.name.localeCompare(b.name, undefined, { sensitivity: 'base' })
      })
      for (const n of nodes) sortNodes(n.children)
    }
    sortNodes(root.children)
    return root.children
  }

  function toggleExpand(path: string) {
    const next = new Set(expanded)
    if (next.has(path)) next.delete(path)
    else next.add(path)
    expanded = next
  }

  function expandDefaults(nodes: TreeNode[], depth = 0) {
    const next = new Set(expanded)
    for (const n of nodes) {
      if (n.is_dir && depth < 1) {
        next.add(n.path)
        expandDefaults(n.children, depth + 1)
      }
    }
    expanded = next
  }

  async function refresh() {
    const id = Number(params.id)
    const [data, libs, fileData] = await Promise.all([
      api.getDownload(id),
      api.myLibraries(),
      api.downloadFiles(id).catch(() => ({
        files: [] as ContentEntry[],
        source: 'none',
        content_path: null,
      })),
    ])
    download = data.download
    libraries = libs.libraries
    files = fileData.files
    filesSource = fileData.source
    contentPath = fileData.content_path ?? null
    if (libraries.length === 1) libraryId = libraries[0].id
    if (download.kind !== 'pack' && !download.map_files) {
      push(`/match/${id}`)
      return
    }
    if (expanded.size === 0 && fileData.files.length) {
      expandDefaults(buildTree(fileData.files))
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

  function selectSource(path: string, isDir: boolean) {
    if (mappedPaths.has(path)) return
    if (isDir) {
      selectedPaths = new Set([path])
      selectedIsDir = true
      selectedPath = path
    } else {
      const next = selectedIsDir ? new Set<string>() : new Set(selectedPaths)
      if (next.has(path)) next.delete(path)
      else next.add(path)
      selectedPaths = next
      selectedIsDir = false
      selectedPath = next.size ? [...next].at(-1)! : null
    }
    const focus = selectedPath
    if (focus) {
      const parsed = parseFolderName(focus)
      title = parsed.title
      author = ''
    } else {
      title = ''
      author = ''
    }
    asin = ''
    matches = []
  }

  function clearSelection() {
    selectedPaths = new Set()
    selectedIsDir = false
    selectedPath = null
    title = ''
    author = ''
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
    if (!selectedPaths.size) {
      showToast('Select a folder or file first')
      return
    }
    if (libraries.length > 1 && !libraryId) {
      showToast('Select which library to import into')
      return
    }
    saving = true
    try {
      const paths = [...selectedPaths]
      const data = await api.mapDownloadItem(Number(params.id), {
        source_paths: paths,
        match_data: m,
        library_id: libraryId ?? undefined,
      })
      download = data.download
      showToast(
        paths.length > 1
          ? `Mapped ${paths.length} files as ${m.title}`
          : `Mapped ${m.title}`,
      )
      clearSelection()
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

  async function unimport(itemId: number, title: string) {
    const label = title || 'this book'
    if (
      !window.confirm(
        `Un-import “${label}”?\n\nThis deletes the copy from your library folder and clears the mapping so you can remap those files.`,
      )
    ) {
      return
    }
    try {
      const data = await api.unimportDownloadItem(Number(params.id), itemId)
      download = data.download
      showToast('Un-imported — paths are free to remap')
    } catch (err) {
      showToast(err instanceof Error ? err.message : 'Could not un-import')
    }
  }

  async function retryFailed() {
    retrying = true
    try {
      const data = await api.retryPackImports(Number(params.id))
      download = data.download
      showToast(`Retrying ${data.retried} import${data.retried === 1 ? '' : 's'}`)
    } catch (err) {
      showToast(err instanceof Error ? err.message : 'Retry failed')
    } finally {
      retrying = false
    }
  }

  async function resetStuckCopy() {
    if (
      !window.confirm(
        'Reset stuck copies?\n\nOnly use this if an import looks frozen (for example after a container restart). Live copies that are still running will be interrupted.',
      )
    ) {
      return
    }
    await retryFailed()
  }

  async function refreshFromQbit() {
    refreshingQbit = true
    try {
      const data = await api.refreshQbittorrent(Number(params.id))
      download = data.download
      contentPath = data.content_path
      const fileData = await api.downloadFiles(Number(params.id)).catch(() => ({
        files: [] as ContentEntry[],
        source: 'none',
        content_path: data.content_path,
      }))
      files = fileData.files
      filesSource = fileData.source
      showToast(
        data.paths_changed
          ? `Paths updated from qBit${data.requeued_items ? ` · requeued ${data.requeued_items}` : ''}`
          : `Synced from qBit (${data.qb_state})${data.requeued_items ? ` · requeued ${data.requeued_items}` : ''}`,
      )
    } catch (err) {
      showToast(err instanceof Error ? err.message : 'qBit refresh failed')
    } finally {
      refreshingQbit = false
    }
  }

  function formatSize(n: number) {
    if (!n) return ''
    const u = ['B', 'KB', 'MB', 'GB']
    const i = Math.min(Math.floor(Math.log(n) / Math.log(1024)), u.length - 1)
    return `${(n / 1024 ** i).toFixed(1)} ${u[i]}`
  }

  function childMappedCount(node: TreeNode): number {
    if (!node.is_dir) return mappedPaths.has(node.path) ? 1 : 0
    let n = mappedPaths.has(node.path) ? 1 : 0
    for (const c of node.children) n += childMappedCount(c)
    return n
  }
</script>

{#if loading || !download}
  <div class="card muted">Loading file map…</div>
{:else}
  <div class="card stack">
    <div class="header-row">
      <div>
        <h2>Map files to Audible</h2>
        <p class="muted">
          {download.name || 'Pack torrent'} ·
          <span class="badge {download.status}">{download.status.replaceAll('_', ' ')}</span>
          · files from {filesSource}
        </p>
        <p class="muted">
          Map a whole folder as one book, or multi-select loose chapter files and map them as one
          title. Imports retry when paths move from incomplete → complete.
        </p>
        {#if contentPath}
          <p class="muted tiny path-hint">Current qBit path: <code>{contentPath}</code></p>
        {/if}
      </div>
      <div class="header-actions">
        <button class="secondary" type="button" disabled={refreshingQbit} onclick={refreshFromQbit}>
          {refreshingQbit ? 'Refreshing…' : 'Refresh from qBit'}
        </button>
        {#if retryableCount > 0}
          <button
            class="secondary"
            type="button"
            disabled={retrying}
            onclick={() => (copyingCount > 0 ? resetStuckCopy() : retryFailed())}
          >
            {retrying
              ? 'Retrying…'
              : copyingCount > 0 && failedCount === 0
                ? `Reset ${copyingCount} stuck`
                : `Retry ${retryableCount} stuck/failed`}
          </button>
        {/if}
        <a class="btn secondary" href="#/">Back to queue</a>
      </div>
    </div>
  </div>

  <div class="map-layout">
    <div class="card stack sources-card">
      <div class="sources-head">
        <h3>Torrent contents</h3>
        <div class="sources-actions">
          <button class="secondary tiny-btn" type="button" disabled={refreshingQbit} onclick={refreshFromQbit}>
            {refreshingQbit ? '…' : 'Refresh qBit'}
          </button>
          <button class="secondary tiny-btn" type="button" onclick={() => refresh()}>Reload list</button>
        </div>
      </div>

      {#if !files.length}
        <p class="muted">No files yet — wait until qBittorrent has metadata, then refresh.</p>
      {:else}
        <div class="tree" role="tree">
          {#each tree as node}
            {@render treeNode(node, 0)}
          {/each}
        </div>
      {/if}

      <h3>Mapped ({download.items?.length || 0})</h3>
      {#if copyingCount > 0}
        <p class="muted tiny">
          Import in progress — if this persists after a restart, use Reset stuck copy.
        </p>
      {/if}
      {#if !download.items?.length}
        <p class="muted">Nothing mapped yet — select a folder or file above.</p>
      {:else}
        <div class="mapped-list">
          {#each download.items as item}
            {@const paths = item.source_paths?.length ? item.source_paths : [item.source_path]}
            <div
              class="mapped-row"
              class:failed={item.status === 'error'}
              class:copying={item.status === 'copying'}
            >
              <div class="mapped-meta-block">
                <strong>{item.metadata?.title || paths[0]}</strong>
                {#each paths as p}
                  <div class="muted path-line">{p}</div>
                {/each}
                <span class="badge {item.status}">{item.status}</span>
                {#if item.error_message}
                  <div class="err">{item.error_message}</div>
                {/if}
              </div>
              <div class="mapped-actions">
                {#if item.status === 'imported'}
                  <button
                    class="danger"
                    type="button"
                    onclick={() => unimport(item.id, item.metadata?.title || paths[0])}
                  >
                    Un-import
                  </button>
                {:else if item.status === 'copying'}
                  <button class="secondary" type="button" disabled={retrying} onclick={resetStuckCopy}>
                    Reset copy
                  </button>
                {:else}
                  <button class="danger" type="button" onclick={() => unmap(item.id)}>Unmap</button>
                {/if}
              </div>
            </div>
          {/each}
        </div>
      {/if}
    </div>

    <div class="card stack">
      <h3>
        Audible match
        {#if selectionCount > 1}
          for <code>{selectionCount} files</code>
        {:else if selectedPath}
          for <code>{selectedPath}</code>
        {/if}
      </h3>
      {#if !selectionCount}
        <p class="muted">Select a folder, or tap files to multi-select chapters.</p>
      {:else}
        {#if selectionCount > 1}
          <p class="map-cta">Map {selectionCount} files as one book</p>
          <ul class="sel-list">
            {#each [...selectedPaths] as p}
              <li><code>{p}</code></li>
            {/each}
          </ul>
          <button class="secondary tiny-btn" type="button" onclick={clearSelection}>Clear selection</button>
        {/if}
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

        <form class="stack match-fields" onsubmit={search}>
          <label>Title
            <input bind:value={title} />
          </label>
          <label>Author
            <input bind:value={author} placeholder="Optional — type to narrow search" />
          </label>
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

{#snippet treeNode(node: TreeNode, depth: number)}
  {@const mapped = mappedPaths.has(node.path)}
  {@const isOpen = expanded.has(node.path)}
  {@const nestedMapped = node.is_dir ? childMappedCount(node) : 0}
  {@const selected = selectedPaths.has(node.path)}
  <div
    class="tree-node"
    style={`--depth:${depth}`}
    role="treeitem"
    aria-selected={selected}
    aria-expanded={node.is_dir ? isOpen : undefined}
  >
    <div
      class="tree-row"
      class:dir={node.is_dir}
      class:file={!node.is_dir}
      class:selected
      class:mapped
    >
      {#if node.is_dir}
        <button
          type="button"
          class="twist"
          aria-label={isOpen ? 'Collapse' : 'Expand'}
          onclick={() => toggleExpand(node.path)}
        >
          {isOpen ? '▾' : '▸'}
        </button>
      {:else}
        <span class="twist spacer" aria-hidden="true"></span>
      {/if}

      <button
        type="button"
        class="pick"
        disabled={mapped}
        onclick={() => selectSource(node.path, node.is_dir)}
      >
        <span class="icon" class:folder={node.is_dir} class:audio={!node.is_dir} aria-hidden="true"></span>
        <span class="label">{node.name}</span>
        {#if node.is_dir}
          <span class="meta">{node.children.length}</span>
          {#if nestedMapped > 0}<span class="meta mapped-meta">{nestedMapped} mapped</span>{/if}
        {:else if node.size}
          <span class="meta">{formatSize(node.size)}</span>
        {/if}
        {#if mapped}<span class="mapped-pill">mapped</span>{/if}
        {#if selected && !mapped && !node.is_dir && selectionCount > 1}
          <span class="mapped-pill">selected</span>
        {/if}
      </button>
    </div>

    {#if node.is_dir && isOpen}
      <div class="tree-children" role="group">
        {#each node.children as child}
          {@render treeNode(child, depth + 1)}
        {/each}
      </div>
    {/if}
  </div>
{/snippet}

<style>
  .header-row {
    display: flex;
    justify-content: space-between;
    gap: 0.75rem;
    align-items: start;
    flex-wrap: wrap;
  }
  .header-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
    justify-content: flex-end;
  }
  .path-hint {
    margin: 0.35rem 0 0;
    word-break: break-all;
  }
  .map-layout {
    display: grid;
    grid-template-columns: 1.1fr 0.9fr;
    gap: 1rem;
    align-items: start;
  }
  .sources-head {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 0.5rem;
    flex-wrap: wrap;
  }
  .sources-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 0.35rem;
  }
  .sources-head h3 {
    margin: 0;
  }
  .tiny-btn {
    padding: 0.35rem 0.65rem !important;
    font-size: 0.82rem;
  }
  .tree {
    border: 1px solid var(--border);
    border-radius: 12px;
    background:
      linear-gradient(180deg, color-mix(in oklab, var(--bg-elevated) 80%, transparent), transparent 8rem),
      var(--bg);
    max-height: min(34rem, 70vh);
    overflow: auto;
    padding: 0.45rem 0.35rem;
  }
  .tree-node {
    --depth: 0;
  }
  .tree-children {
    border-left: 1px dashed color-mix(in oklab, var(--border) 80%, transparent);
    margin-left: calc(0.85rem + (var(--depth) * 0.05rem));
  }
  .tree-row {
    display: grid;
    grid-template-columns: 1.5rem 1fr;
    gap: 0.15rem;
    align-items: stretch;
    padding-left: calc(var(--depth) * 0.85rem);
  }
  .twist {
    width: 1.5rem;
    height: 2rem;
    padding: 0 !important;
    border: none !important;
    background: transparent !important;
    color: var(--muted) !important;
    font-size: 0.85rem;
    line-height: 1;
  }
  .twist.spacer {
    display: inline-block;
  }
  .pick {
    display: flex;
    align-items: center;
    gap: 0.45rem;
    text-align: left;
    min-width: 0;
    background: transparent !important;
    color: var(--text) !important;
    border: 1px solid transparent !important;
    border-radius: 8px;
    padding: 0.4rem 0.55rem !important;
    font-weight: 500;
  }
  .tree-row.dir .pick {
    background: color-mix(in oklab, var(--accent) 6%, transparent) !important;
  }
  .tree-row.file .pick {
    background: color-mix(in oklab, var(--bg-elevated) 55%, transparent) !important;
  }
  .tree-row.file .label {
    font-family: var(--mono);
    font-size: 0.86rem;
    font-weight: 500;
  }
  .tree-row.dir .label {
    font-weight: 700;
  }
  .tree-row.selected .pick {
    border-color: var(--accent) !important;
    background: color-mix(in oklab, var(--accent) 16%, transparent) !important;
  }
  .tree-row.mapped .pick {
    opacity: 0.55;
  }
  .icon {
    flex: 0 0 auto;
    width: 1.05rem;
    height: 0.9rem;
    border-radius: 2px;
    position: relative;
  }
  .icon.folder {
    background: color-mix(in oklab, var(--accent) 55%, #c9a227);
    border-radius: 2px 2px 3px 3px;
    box-shadow: inset 0 0.22rem 0 0 color-mix(in oklab, var(--accent) 35%, white);
  }
  .icon.audio {
    width: 0.85rem;
    height: 0.85rem;
    border-radius: 50%;
    border: 2px solid var(--muted);
    background: transparent;
  }
  .icon.audio::after {
    content: '';
    position: absolute;
    left: 0.28rem;
    top: 0.12rem;
    width: 0.18rem;
    height: 0.45rem;
    border-radius: 1px;
    background: var(--muted);
  }
  .label {
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .meta {
    flex: 0 0 auto;
    font-size: 0.72rem;
    color: var(--muted);
    font-weight: 600;
  }
  .mapped-meta {
    color: var(--accent);
  }
  .mapped-pill {
    flex: 0 0 auto;
    font-size: 0.68rem;
    font-weight: 700;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--accent);
    border: 1px solid color-mix(in oklab, var(--accent) 40%, var(--border));
    border-radius: 999px;
    padding: 0.1rem 0.4rem;
  }
  .mapped-list {
    display: grid;
    gap: 0.55rem;
  }
  .mapped-row {
    display: flex;
    flex-direction: column;
    gap: 0.65rem;
    align-items: stretch;
    border: 1px solid var(--border);
    border-radius: 10px;
    padding: 0.65rem 0.75rem;
    background: var(--bg);
  }
  .mapped-row.failed {
    border-color: color-mix(in oklab, var(--danger) 45%, var(--border));
  }
  .mapped-row.copying {
    border-color: color-mix(in oklab, var(--accent) 35%, var(--border));
  }
  .mapped-meta-block {
    min-width: 0;
  }
  .mapped-meta-block strong {
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
    line-height: 1.3;
  }
  .mapped-meta-block .badge {
    margin-top: 0.35rem;
  }
  .mapped-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
    justify-content: flex-end;
    width: 100%;
    padding-top: 0.55rem;
    border-top: 1px solid var(--border);
  }
  .mapped-actions button {
    flex: 0 0 auto;
    white-space: nowrap;
  }
  @media (max-width: 520px) {
    .mapped-actions {
      justify-content: stretch;
    }
    .mapped-actions button {
      flex: 1 1 auto;
      justify-content: center;
      text-align: center;
    }
  }
  .path-line {
    font-family: var(--mono);
    font-size: 0.78rem;
    word-break: break-all;
    margin: 0.2rem 0 0.15rem;
  }
  .map-cta {
    font-weight: 700;
    margin: 0;
    color: var(--accent);
  }
  .sel-list {
    margin: 0;
    padding-left: 1.1rem;
    font-size: 0.82rem;
    max-height: 8rem;
    overflow: auto;
  }
  .err {
    color: var(--danger);
    font-size: 0.85rem;
    margin-top: 0.25rem;
    word-break: break-word;
  }
  .match-fields {
    gap: 0.65rem;
  }
  .match-grid.compact {
    grid-template-columns: repeat(auto-fill, minmax(140px, 1fr));
  }
  code {
    font-family: var(--mono);
    font-size: 0.85em;
  }
  .tiny {
    font-size: 0.8rem;
  }
  @media (max-width: 900px) {
    .map-layout {
      grid-template-columns: 1fr;
    }
    .tree {
      max-height: 22rem;
    }
    .header-actions {
      width: 100%;
    }
    .header-actions > * {
      flex: 1 1 calc(50% - 0.25rem);
      text-align: center;
      justify-content: center;
    }
    .header-actions a.btn {
      display: inline-flex;
    }
  }
  @media (max-width: 480px) {
    .header-actions > * {
      flex: 1 1 100%;
    }
  }
</style>
