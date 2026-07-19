<script lang="ts">
  import { onMount } from 'svelte'
  import { push } from 'svelte-spa-router'
  import { api, type Library, type User } from '../lib/api'
  import { canStartDownloads, isRequester } from '../lib/roles'
  import { currentUser } from '../lib/session'
  import { showToast } from '../lib/toast'

  let { params = { id: '' } }: { params?: { id: string } } = $props()

  type Kind = 'single' | 'pack' | null

  let kind = $state<Kind>(null)
  let title = $state('')
  let author = $state('')
  let asin = $state('')
  let matches = $state<any[]>([])
  let selected = $state<any | null>(null)
  let searching = $state(false)
  let saving = $state(false)
  let loading = $state(true)
  let displayName = $state<string | null>(null)
  let libraries = $state<Library[]>([])
  let libraryId = $state<number | null>(null)
  let downloadKind = $state('single')
  let user = $state<User | null>(null)

  currentUser.subscribe((v) => (user = v))

  function stripExtension(name: string) {
    return name.replace(/\.(m4b|m4a|mp3|flac|ogg|opus|aac|wma|wav|mp4|mka|pdf|epub)$/i, '')
  }

  function parseName(name: string | null | undefined) {
    if (!name) return { title: '', author: '' }
    return { title: stripExtension(name.trim()), author: '' }
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
      downloadKind = data.download.kind || 'single'
      if (
        (downloadKind === 'pack' || data.download.map_files) &&
        data.download.status !== 'awaiting_match' &&
        data.download.status !== 'rejected'
      ) {
        push(`/map/${params.id}`)
        return
      }
      if (downloadKind === 'pack') kind = 'pack'

      const parsed = parseName(data.download.name)
      title = parsed.title
      author = parsed.author
    } catch (err) {
      showToast(err instanceof Error ? err.message : 'Could not load download')
    } finally {
      loading = false
    }
  })

  async function chooseSingle() {
    kind = 'single'
    selected = null
    if (title) await search()
  }

  async function search(e?: Event) {
    e?.preventDefault()
    searching = true
    selected = null
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

  function pickMatch(m: any) {
    if (libraries.length > 1 && !libraryId) {
      showToast('Select which library to import into')
      return
    }
    selected = m
  }

  async function confirmMatch(mapFiles: boolean) {
    if (!selected) return
    saving = true
    try {
      const res = await api.matchDownload(
        Number(params.id),
        selected,
        libraryId ?? undefined,
        mapFiles,
      )
      if (res.pending_approval) {
        showToast('Matched — waiting for approval to start download')
        push('/')
      } else if (mapFiles) {
        showToast('Matched — downloading, then map files')
        push(`/map/${params.id}`)
      } else {
        showToast('Matched and sent to qBittorrent')
        push('/')
      }
    } catch (err) {
      showToast(err instanceof Error ? err.message : 'Match failed')
    } finally {
      saving = false
    }
  }

  async function startPack() {
    saving = true
    try {
      const res = await api.startPack(Number(params.id))
      if (res.pending_approval) {
        showToast('Pack submitted for approval')
        push('/')
      } else {
        showToast('Pack downloading — map books once files appear')
        push(`/map/${params.id}`)
      }
    } catch (err) {
      showToast(err instanceof Error ? err.message : 'Could not start pack')
    } finally {
      saving = false
    }
  }
</script>

{#if loading}
  <div class="card muted">Loading match…</div>
{:else if kind === null}
  <div class="card stack">
    <div>
      <h2>How should this torrent be handled?</h2>
      {#if displayName}
        <p class="torrent-name muted"><span class="label">Torrent</span> {displayName}</p>
      {/if}
      <p class="muted">
        Single books match one Audible title before download. Packs download first, then you map folders to titles.
      </p>
    </div>
    {#if isRequester(user)}
      <div class="callout">
        You’re a <strong>requester</strong>. Match Audible first — an approver starts the download afterward.
      </div>
    {/if}
    <div class="kind-grid">
      <button type="button" class="kind-card" disabled={libraries.length === 0} onclick={chooseSingle}>
        <span class="kind-kicker">Single</span>
        <strong>Single book</strong>
        <span class="muted">
          Match Audible → {isRequester(user) ? 'await approval' : 'download'} → import whole torrent
        </span>
      </button>
      <button type="button" class="kind-card" disabled={libraries.length === 0} onclick={() => (kind = 'pack')}>
        <span class="kind-kicker">Pack</span>
        <strong>Pack / collection</strong>
        <span class="muted">
          {isRequester(user)
            ? 'Submit for approval → download → map each folder'
            : 'Download now → map each folder or file to Audible'}
        </span>
      </button>
    </div>
    {#if libraries.length === 0}
      <div class="banner-warn">No libraries assigned to your account. Ask an admin to grant access.</div>
    {/if}
  </div>
{:else if kind === 'pack'}
  <div class="card stack">
    <div>
      <h2>Pack / collection</h2>
      <p class="muted">
        {#if isRequester(user)}
          Submits this pack for approval. After it’s started, map each folder or file to an Audible title.
        {:else}
          Starts the torrent without an Audible match. When files appear, open <strong>Map</strong> to assign titles.
        {/if}
      </p>
    </div>
    {#if libraries.length === 0}
      <div class="banner-warn">No libraries assigned to your account.</div>
    {/if}
    <div class="action-row">
      <button type="button" disabled={saving || libraries.length === 0} onclick={startPack}>
        {#if saving}
          Submitting…
        {:else if isRequester(user)}
          Submit pack for approval
        {:else}
          Start pack download
        {/if}
      </button>
      <button class="secondary" type="button" disabled={saving} onclick={() => (kind = null)}>Back</button>
    </div>
  </div>
{:else}
  <div class="card stack">
    <div>
      <h2>Match Audible metadata</h2>
      <p class="muted">
        Search Audible’s catalog, then enrich via Audnexus.
        {#if displayName}
          Prefilling from <strong>{displayName}</strong>.
        {/if}
      </p>
      <button class="linkish" type="button" onclick={() => { kind = null; selected = null }}>
        Change to pack / collection
      </button>
    </div>

    {#if isRequester(user)}
      <div class="callout">
        After you pick a match, the request waits for approval before qBittorrent starts.
      </div>
    {/if}

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
      <p class="muted lib-line">
        Library: <strong>{libraries[0].name}</strong>
        <span class="path">({libraries[0].path})</span>
      </p>
    {/if}

    <form class="stack match-fields" onsubmit={search}>
      <div class="field-row">
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

  {#if selected}
    <div class="card stack confirm-card">
      <div class="confirm-meta">
        <img src={selected.cover_url || '/icons/icon-192.png'} alt="" />
        <div class="confirm-copy">
          <strong>{selected.title}</strong>
          <div class="muted">{(selected.authors || []).join(', ')}</div>
          <div class="muted asin">ASIN {selected.asin}</div>
        </div>
      </div>
      <p class="muted confirm-hint">
        {#if isRequester(user)}
          Choose how this should be handled after an approver starts the download.
        {:else}
          Choose how to handle the torrent after matching.
        {/if}
      </p>
      <div class="mode-grid">
        {#if isRequester(user)}
          <button type="button" class="mode-card" disabled={saving} onclick={() => confirmMatch(false)}>
            <strong>{saving ? 'Submitting…' : 'Submit for approval'}</strong>
            <span class="muted">Auto-import the whole torrent after download</span>
          </button>
          <button type="button" class="mode-card" disabled={saving} onclick={() => confirmMatch(true)}>
            <strong>Submit · map files later</strong>
            <span class="muted">Download first, then pick which files to import</span>
          </button>
        {:else if canStartDownloads(user)}
          <button type="button" class="mode-card" disabled={saving} onclick={() => confirmMatch(false)}>
            <strong>{saving ? 'Starting…' : 'Start download'}</strong>
            <span class="muted">Match → download → import whole torrent</span>
          </button>
          <button type="button" class="mode-card" disabled={saving} onclick={() => confirmMatch(true)}>
            <strong>Download then map files</strong>
            <span class="muted">Download first, then pick which files to import</span>
          </button>
        {/if}
      </div>
      <button class="secondary back-btn" type="button" disabled={saving} onclick={() => (selected = null)}>
        Back to results
      </button>
    </div>
  {:else if matches.length}
    <div class="match-grid">
      {#each matches as m}
        <button class="match-card" type="button" disabled={saving || libraries.length === 0} onclick={() => pickMatch(m)}>
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
  .torrent-name {
    margin: 0.35rem 0 0.5rem;
    word-break: break-word;
  }
  .torrent-name .label {
    display: inline-block;
    margin-right: 0.35rem;
    font-size: 0.72rem;
    font-weight: 700;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--accent);
  }
  .callout {
    border: 1px solid color-mix(in oklab, var(--accent) 35%, var(--border));
    background: color-mix(in oklab, var(--accent) 10%, transparent);
    border-radius: var(--radius);
    padding: 0.75rem 0.9rem;
    font-size: 0.92rem;
    color: var(--text);
  }
  .banner-warn {
    border: 1px solid color-mix(in oklab, var(--warning) 45%, var(--border));
    color: var(--warning);
    border-radius: var(--radius);
    padding: 0.75rem 0.9rem;
    background: color-mix(in oklab, var(--warning) 10%, transparent);
  }
  .kind-grid,
  .mode-grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
    gap: 0.75rem;
  }
  .kind-card,
  .mode-card {
    display: grid;
    gap: 0.35rem;
    text-align: left;
    padding: 1rem !important;
    background: var(--bg) !important;
    color: var(--text) !important;
    border: 1px solid var(--border) !important;
    border-radius: var(--radius);
    font-weight: 400;
  }
  .kind-card:hover:not(:disabled),
  .mode-card:hover:not(:disabled) {
    border-color: var(--accent) !important;
    background: color-mix(in oklab, var(--accent) 8%, var(--bg)) !important;
  }
  .kind-kicker {
    font-size: 0.7rem;
    font-weight: 700;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--accent);
  }
  .linkish {
    background: none !important;
    border: none !important;
    color: var(--accent) !important;
    padding: 0 !important;
    margin-top: 0.35rem;
    font-weight: 600;
    cursor: pointer;
  }
  .match-fields {
    gap: 0.7rem;
  }
  .field-row {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 0.75rem;
  }
  .lib-line .path {
    word-break: break-all;
  }
  .action-row {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
  }
  .confirm-card {
    border-color: color-mix(in oklab, var(--accent) 30%, var(--border));
  }
  .confirm-meta {
    display: flex;
    gap: 0.85rem;
    align-items: start;
  }
  .confirm-meta img {
    width: 80px;
    height: 80px;
    object-fit: cover;
    border-radius: 8px;
    background: var(--bg);
    border: 1px solid var(--border);
    flex: 0 0 auto;
  }
  .confirm-copy {
    min-width: 0;
  }
  .confirm-copy strong {
    display: block;
    line-height: 1.25;
  }
  .asin {
    margin-top: 0.2rem;
    font-family: var(--mono);
    font-size: 0.82rem;
  }
  .confirm-hint {
    margin: 0;
  }
  .back-btn {
    justify-self: start;
    width: fit-content;
  }
  @media (max-width: 640px) {
    .field-row {
      grid-template-columns: 1fr;
    }
    .confirm-meta img {
      width: 64px;
      height: 64px;
    }
    .action-row button,
    .mode-grid .mode-card {
      width: 100%;
    }
  }
</style>
