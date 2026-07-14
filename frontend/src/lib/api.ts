export type User = {
  id: number
  username: string
  role: string
  must_change_password: boolean
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
  metadata?: {
    asin: string
    title: string
    authors: string[]
    series?: string | null
    series_index?: string | null
    cover_url?: string | null
  } | null
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
  createUser: (username: string, password: string) =>
    request('/users', { method: 'POST', body: JSON.stringify({ username, password }) }),
  deleteUser: (id: number) => request(`/users/${id}`, { method: 'DELETE' }),
  getSettings: () => request<{ settings: Record<string, unknown> }>('/settings'),
  updateSettings: (body: Record<string, unknown>) =>
    request('/settings', { method: 'PUT', body: JSON.stringify(body) }),
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
  createDownload: (input: string, name?: string) =>
    request<{ download: Download }>('/downloads', {
      method: 'POST',
      body: JSON.stringify({ input, name }),
    }),
  deleteDownload: (id: number) => request(`/downloads/${id}`, { method: 'DELETE' }),
  matchDownload: (id: number, match_data: unknown) =>
    request(`/downloads/${id}/match`, {
      method: 'POST',
      body: JSON.stringify({ match_data }),
    }),
  searchMetadata: (title: string, author?: string) => {
    const q = new URLSearchParams({ title })
    if (author) q.set('author', author)
    return request<{ matches: unknown[] }>(`/metadata/search?${q}`)
  },
  metadataByAsin: (asin: string) =>
    request<{ match: unknown }>(`/metadata/asin/${encodeURIComponent(asin)}`),
  abbSearch: (q: string, page = 1) =>
    request<{
      results: unknown[]
      page: number
      has_more: boolean
      mirror?: string
    }>(`/abb/search?q=${encodeURIComponent(q)}&page=${page}`),
  abbDetails: (url: string) =>
    request<{ details: { info_hash?: string; magnet_uri?: string; title?: string } }>(
      `/abb/details?url=${encodeURIComponent(url)}`,
    ),
  subscribePush: (subscription: PushSubscriptionJSON) =>
    request('/push/subscribe', { method: 'POST', body: JSON.stringify(subscription) }),
}
