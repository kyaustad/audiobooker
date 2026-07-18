export type User = {
  id: number
  username: string
  role: string
  must_change_password: boolean
  abs_user_id?: string | null
  libraries?: Library[]
  library_ids?: number[]
  rate_limit_requests?: number | null
  rate_limit_window_secs?: number | null
  rate_limit_active_torrents?: number | null
}

export type NotificationPrefs = {
  notify_imported: boolean
  notify_download_finished: boolean
  notify_pack_ready: boolean
  notify_failures: boolean
}

export type Library = {
  id: number
  name: string
  path: string
  abs_id?: string | null
  abs_path?: string | null
  created_at?: string
}

export type DownloadItem = {
  id: number
  download_id: number
  source_path: string
  source_paths?: string[]
  library_id: number
  status: string
  destination_path?: string | null
  error_message?: string | null
  metadata?: Download['metadata']
}

export type ContentEntry = {
  path: string
  name: string
  is_dir: boolean
  size: number
}

export type Download = {
  id: number
  user_id: number
  magnet_uri: string
  info_hash: string
  name: string | null
  status: string
  progress: number
  download_speed: number
  eta: number
  destination_path: string | null
  error_message: string | null
  library_id?: number | null
  kind?: string
  metadata?: {
    asin: string
    title: string
    authors: string[]
    series?: string | null
    series_index?: string | null
    cover_url?: string | null
  } | null
  items?: DownloadItem[]
}

async function request<T>(path: string, init?: RequestInit): Promise<T> {
  const res = await fetch(`/api${path}`, {
    credentials: 'include',
    headers: {
      'Content-Type': 'application/json',
      ...(init?.headers || {}),
    },
    ...init,
  })
  const data = await res.json().catch(() => ({}))
  if (!res.ok) {
    throw new Error(data.error || `Request failed (${res.status})`)
  }
  return data as T
}

export const api = {
  setupStatus: () => request<{ needs_setup: boolean }>('/setup/status'),
  setup: (
    username: string,
    password: string,
    qb?: {
      qbittorrent_url?: string
      qbittorrent_username?: string
      qbittorrent_password?: string
    },
  ) =>
    request('/setup', {
      method: 'POST',
      body: JSON.stringify({ username, password, ...qb }),
    }),
  testQbitSetup: (body: {
    qbittorrent_url: string
    qbittorrent_username?: string
    qbittorrent_password?: string
  }) =>
    request('/setup/test-qbittorrent', { method: 'POST', body: JSON.stringify(body) }),
  login: (username: string, password: string) =>
    request<{ user: User }>('/auth/login', {
      method: 'POST',
      body: JSON.stringify({ username, password }),
    }),
  logout: () => request('/auth/logout', { method: 'POST' }),
  me: () => request<{ user: User }>('/auth/me'),
  changePassword: (current_password: string, new_password: string) =>
    request('/auth/password', {
      method: 'PUT',
      body: JSON.stringify({ current_password, new_password }),
    }),
  listUsers: () => request<{ users: User[] }>('/users'),
  createUser: (username: string, password: string, library_ids?: number[]) =>
    request('/users', {
      method: 'POST',
      body: JSON.stringify({ username, password, library_ids }),
    }),
  updateUser: (
    id: number,
    body: {
      password?: string
      library_ids?: number[]
      must_change_password?: boolean
      rate_limit_requests?: number | null
      rate_limit_window_secs?: number | null
      rate_limit_active_torrents?: number | null
    },
  ) => request(`/users/${id}`, { method: 'PUT', body: JSON.stringify(body) }),
  deleteUser: (id: number) => request(`/users/${id}`, { method: 'DELETE' }),
  listLibraries: () => request<{ libraries: Library[] }>('/libraries'),
  myLibraries: () => request<{ libraries: Library[] }>('/libraries/mine'),
  createLibrary: (name: string, path: string) =>
    request('/libraries', { method: 'POST', body: JSON.stringify({ name, path }) }),
  updateLibrary: (id: number, name: string, path: string) =>
    request(`/libraries/${id}`, { method: 'PUT', body: JSON.stringify({ name, path }) }),
  deleteLibrary: (id: number) => request(`/libraries/${id}`, { method: 'DELETE' }),
  syncAbsLibraries: (body?: { audiobookshelf_url?: string; audiobookshelf_token?: string }) =>
    request<{ imported: number; needs_path?: number; libraries: Library[] }>(
      '/libraries/sync-abs',
      {
        method: 'POST',
        body: JSON.stringify(body ?? {}),
      },
    ),
  syncAbsUsers: () =>
    request<{
      created: number
      linked: number
      updated_libraries: number
      skipped: number
      total_abs_users: number
      settings: Record<string, unknown>
    }>('/settings/sync-abs-users', { method: 'POST' }),
  getSettings: () => request<{ settings: Record<string, unknown> }>('/settings'),
  updateSettings: (body: Record<string, unknown>) =>
    request<{ settings: Record<string, unknown> }>('/settings', {
      method: 'PUT',
      body: JSON.stringify(body),
    }),
  testQbit: (body?: {
    qbittorrent_url?: string
    qbittorrent_username?: string
    qbittorrent_password?: string
  }) =>
    request('/settings/test-qbittorrent', {
      method: 'POST',
      body: JSON.stringify(body ?? {}),
    }),
  ensureVapid: () => request<{ vapid_public_key: string }>('/push/vapid'),
  apiKeyInfo: () => request<{ configured: boolean; key_prefix: string }>('/api-key'),
  rotateApiKey: () =>
    request<{ api_key: string; warning: string }>('/api-key', { method: 'POST' }),
  listDownloads: () => request<{ downloads: Download[] }>('/downloads'),
  getDownload: (id: number) => request<{ download: Download }>(`/downloads/${id}`),
  createDownload: (input: string, name?: string, kind?: 'single' | 'pack') =>
    request<{ download: Download }>('/downloads', {
      method: 'POST',
      body: JSON.stringify({ input, name, kind }),
    }),
  deleteDownload: (id: number) => request(`/downloads/${id}`, { method: 'DELETE' }),
  matchDownload: (id: number, match_data: unknown, library_id?: number) =>
    request(`/downloads/${id}/match`, {
      method: 'POST',
      body: JSON.stringify({ match_data, library_id }),
    }),
  startPack: (id: number) =>
    request<{ download: Download }>(`/downloads/${id}/start-pack`, { method: 'POST' }),
  downloadFiles: (id: number) =>
    request<{
      files: ContentEntry[]
      source: string
      content_path?: string | null
      save_path?: string | null
    }>(`/downloads/${id}/files`),
  retryPackImports: (id: number) =>
    request<{ retried: number; download: Download }>(`/downloads/${id}/retry-imports`, {
      method: 'POST',
    }),
  refreshQbittorrent: (id: number) =>
    request<{
      ok: boolean
      save_path: string
      content_path: string
      progress: number
      qb_state: string
      requeued_items: number
      paths_changed: boolean
      download: Download
    }>(`/downloads/${id}/refresh-qbittorrent`, { method: 'POST' }),
  mapDownloadItem: (
    id: number,
    body: {
      source_path?: string
      source_paths?: string[]
      match_data: unknown
      library_id?: number
    },
  ) =>
    request<{ download: Download }>(`/downloads/${id}/items`, {
      method: 'POST',
      body: JSON.stringify(body),
    }),
  unmapDownloadItem: (id: number, itemId: number) =>
    request<{ download: Download }>(`/downloads/${id}/items/${itemId}`, { method: 'DELETE' }),
  unimportDownloadItem: (id: number, itemId: number) =>
    request<{ download: Download }>(`/downloads/${id}/items/${itemId}/unimport`, {
      method: 'POST',
    }),
  searchMetadata: (title: string, author?: string) => {
    const q = new URLSearchParams({ title })
    if (author) q.set('author', author)
    return request<{ matches: unknown[] }>(`/metadata/search?${q}`)
  },
  metadataByAsin: (asin: string) =>
    request<{ match: unknown }>(`/metadata/asin/${encodeURIComponent(asin)}`),
  abbCategories: () => request<{ categories: AbbCategory[] }>('/abb/categories'),
  abbBrowse: (page = 1, category?: string) => {
    const q = new URLSearchParams({ page: String(page) })
    if (category) q.set('category', category)
    return request<{
      results: AbbSearchResult[]
      page: number
      has_more: boolean
      mirror?: string
      mode?: string
      query?: string | null
      category?: string | null
      category_label?: string | null
    }>(`/abb/browse?${q}`)
  },
  abbSearch: (q: string, page = 1) =>
    request<{
      results: AbbSearchResult[]
      page: number
      has_more: boolean
      mirror?: string
      mode?: string
      query?: string | null
      category?: string | null
      category_label?: string | null
    }>(`/abb/search?q=${encodeURIComponent(q)}&page=${page}`),
  abbDetails: (url: string) =>
    request<{ details: AbbDetails }>(`/abb/details?url=${encodeURIComponent(url)}`),
  pushStatus: () =>
    request<{
      subscribed: boolean
      subscriptions: number
      preferences: NotificationPrefs
    }>('/push/status'),
  updatePushPreferences: (preferences: NotificationPrefs) =>
    request<{ ok: boolean; preferences: NotificationPrefs }>('/push/preferences', {
      method: 'PUT',
      body: JSON.stringify(preferences),
    }),
  subscribePush: (subscription: PushSubscriptionJSON) =>
    request('/push/subscribe', { method: 'POST', body: JSON.stringify(subscription) }),
  unsubscribePush: (endpoint?: string) =>
    request('/push/unsubscribe', {
      method: 'POST',
      body: JSON.stringify({ endpoint }),
    }),
  testPush: () => request('/push/test', { method: 'POST' }),
}

export type AbbCategory = {
  slug: string
  label: string
  group: string
}

export type AbbSearchResult = {
  title: string
  url: string
  cover_url?: string | null
  info?: string | null
  author?: string | null
  language?: string | null
  format?: string | null
  bitrate?: string | null
  size?: string | null
  posted?: string | null
  category?: string | null
}

export type AbbDetails = {
  title: string
  url: string
  info_hash?: string | null
  magnet_uri?: string | null
  cover_url?: string | null
  description?: string | null
  author?: string | null
  narrator?: string | null
  format?: string | null
  bitrate?: string | null
  size?: string | null
}
