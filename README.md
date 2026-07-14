# Audiobooker

Low-resource audiobook download manager for [Audiobookshelf](https://www.audiobookshelf.org/), inspired by the *arr suite.

**Stack:** Rust (Axum) backend + Svelte SPA, SQLite, single Docker process.

```
AudiobookBay → Audiobooker → qBittorrent → copy Author/Series/Book → Audiobookshelf
                 ↑
            Audible match (Audnexus)
```

## Workflow

1. First boot → create **root** admin (optional qBittorrent test)
2. Root configures **qBittorrent** + library paths in **Settings**
3. Root creates **users** (temporary passwords)
4. Users add magnets/hashes (or browse AudiobookBay), **match Audible metadata**, then download
5. On completion, files are **copied** into `Author/Series/Title` for Audiobookshelf
6. Optional PWA push when a book is imported

## Docker (recommended)

Published image (after CI runs on `main`):

```bash
docker pull ghcr.io/kyaustad/audiobooker:latest
```

Package page: [ghcr.io/kyaustad/audiobooker](https://github.com/kyaustad/audiobooker/pkgs/container/audiobooker)

### docker compose

```bash
git clone https://github.com/kyaustad/audiobooker.git
cd audiobooker
cp .env.example .env
# Edit host paths in .env (see below)
docker compose up -d
```

Or pull the published image without building:

```yaml
# swap `build: .` for:
image: ghcr.io/kyaustad/audiobooker:latest
```

Open `http://server:3000`, complete root setup, then finish qBittorrent + paths under **Settings**.

### Volumes

| Container path | Purpose | Host tip (Unraid) |
|----------------|---------|-------------------|
| `/data` | SQLite DB + app state | e.g. `/mnt/user/appdata/audiobooker` |
| `/downloads` | Completed torrent files (read-only OK) | Same share qBittorrent writes to |
| `/audiobooks` | Library root Audiobookshelf reads | Your ABS library share |

`docker-compose.yml` defaults:

| Env var | Default | Maps to |
|---------|---------|---------|
| `TORRENT_DOWNLOADS_PATH` | `/mnt/user/downloads` | → `/downloads` |
| `AUDIOBOOK_LIBRARY_PATH` | `/mnt/user/audiobooks` | → `/audiobooks` |
| `HOST_PORT` | `3000` | host port → `3000` |
| `COOKIE_SECURE` | `false` | set `true` only behind HTTPS |
| `PUID` / `PGID` | `0` / `0` | optional non-root user |

### Settings paths (important)

In the UI, set paths **as the container sees them**, not host paths:

| Setting | Typical value |
|---------|----------------|
| Download path | `/downloads` |
| Library path | `/audiobooks` |
| qBittorrent URL | `http://qbittorrent:8080` or LAN IP of the WebUI |

Audiobooker copies from the path qBittorrent reports. If qBit uses a different in-container path (e.g. `/mnt/user/downloads`), set **Download path** to `/downloads` so Audiobooker can remap using the torrent’s save path.

**Best setup:** mount the same host folder into both containers at the **same** container path (`/downloads`).

### Permissions

- Default: container runs as root (`PUID=0`), which avoids write failures on first boot.
- On Unraid, Audiobookshelf may not read root-owned imports. Either:
  - set `PUID=99` and `PGID=100` (common Unraid share ownership), **and** `chown -R 99:100` your `./data` (or appdata) folder before start, or
  - leave root and run a periodic New Permissions / `chown` on the library share.
- `/data` must be writable by the container user or the DB cannot be created.
- `/audiobooks` must be writable or imports fail after download completes.
- `/downloads` can be `:ro` if you only copy out of it.

### Unraid (Docker UI)

1. Repository: `ghcr.io/kyaustad/audiobooker:latest`
2. Port: `3000` → host of your choice
3. Paths:
   - `/data` → `appdata/audiobooker`
   - `/downloads` → your qBittorrent completed folder (**same share**)
   - `/audiobooks` → Audiobookshelf library
4. Extra: `COOKIE_SECURE=false` on HTTP LAN; `true` if reverse-proxied with TLS
5. Optional: `PUID` / `PGID` matching the share

If GHCR asks to log in for a public image, make sure the package visibility is **public** (Actions sets this after the first successful publish; you can also change it under the package settings on GitHub).

## Common snags

| Symptom | Likely cause |
|---------|----------------|
| Redirected to login forever / session lost | Serving over HTTP with `COOKIE_SECURE=true` — set `false` on plain HTTP |
| “Source path does not exist” after complete | `/downloads` not mapped to the same files qBit finished, or Settings → Download path wrong |
| Import permission denied | Library mount not writable by container user (PUID/PGID / root ownership) |
| qBittorrent connection test fails | Wrong WebUI URL from inside Docker (`localhost` is the Audiobooker container). Use bridge DNS name or host LAN IP; enable WebUI auth if required |
| Can’t pull from GHCR | Package still private — open the package on GitHub → Package settings → Change visibility → Public |
| AudiobookBay browse empty / pagination odd | Mirrors/layout change; try a broader query. Narrow searches may only have one page |

## Roles

| Role | Capabilities |
|------|----------------|
| **root** | Settings, users, API key (no personal queue) |
| **user** | Queue, Audible match, ABB browse, password change, PWA notifications |

## API key (*arr-style*)

Root → **API Key** → generate/rotate. Header:

```http
X-Api-Key: abk_...
```

| Method | Path | Description |
|--------|------|-------------|
| `POST` | `/api/v1/user` | Create user `{ "username", "password" }` |
| `GET` | `/api/v1/queue` | All downloads |
| `GET` | `/api/v1/queue/{username}` | One user’s queue |

## CI / GHCR

Pushing to `main` (or a `v*` tag) runs [`.github/workflows/docker-publish.yml`](.github/workflows/docker-publish.yml):

- Builds the multi-stage Dockerfile
- Pushes `ghcr.io/<owner>/<repo>:latest` (on default branch), semver tags, and `sha-*`
- Attaches provenance / SBOM
- Links the package to this repository and attempts to set visibility to **public**

Manual rebuild: Actions → **Publish Docker image** → Run workflow.

## Local development

Requirements: Rust stable, Node 22+, pnpm.

```bash
# Terminal 1 — API
cd backend
mkdir -p static data
cargo run

# Terminal 2 — SPA (proxies /api → :3000)
cd frontend
pnpm install
pnpm dev
```

## Notes

- Metadata: Audible catalog search + Audnexus enrichment (default `https://api.audnex.us`)
- AudiobookBay browse (`#/browse`) is a convenience scraper against mirrors (`.lu`, `.fi`); site changes can break parsing
- Cookies default to non-secure for LAN HTTP

## License

See [LICENSE](LICENSE).
