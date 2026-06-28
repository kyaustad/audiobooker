# Audiobooker

A self-hosted web app for downloading audiobooks through qBittorrent and copying finished files into your library folder for [Audiobookshelf](https://www.audiobookshelf.org/) (or similar tools) to pick up.

It is designed around a simple workflow:

1. Find an audiobook on **AudiobookBay** (or another indexer) and copy the **info hash**
2. Paste the hash (or a full magnet link) into Audiobooker
3. Audiobooker sends the torrent to **qBittorrent** and tracks progress per user
4. When the download completes, files are **copied** (not moved) into your configured audiobook folder

## Features

- User accounts with SQLite-backed auth
- Add downloads via magnet link **or** raw 40-character info hash
- Per-user download history and progress tracking
- Automatic sync with qBittorrent (progress, speed, ETA)
- Automatic file copy on completion to your library path
- Docker-ready for Unraid and other homelab setups

## How it works

```
AudiobookBay  →  copy info hash  →  Audiobooker  →  qBittorrent  →  copy to library  →  Audiobookshelf
```

Audiobooker does not scrape AudiobookBay. You browse there as usual, grab the hash from a torrent page, and paste it here. If you only have the hash, the app builds a magnet link for you (similar to [hashtomagnet.com](https://hashtomagnet.com)).

Each user only sees their own downloads. A background job polls qBittorrent every few seconds and updates the database. When a torrent reaches 100%, the app copies the content from the qBittorrent download path to `AUDIOBOOK_DEST_PATH`.

## Quick start (Docker)

### 1. Configure environment

```bash
cp .env.example .env
```

Edit `.env` with your values:

| Variable | Description |
|----------|-------------|
| `SESSION_SECRET` | Long random string for session encryption |
| `QBITTORRENT_URL` | qBittorrent Web UI URL (e.g. `http://qbittorrent:8080`) |
| `QBITTORRENT_USERNAME` | qBittorrent Web UI username |
| `QBITTORRENT_PASSWORD` | qBittorrent Web UI password |
| `TORRENT_DOWNLOADS_PATH` | Host path where qBittorrent saves files |
| `AUDIOBOOK_LIBRARY_PATH` | Host path for your Audiobookshelf library |

### 2. Start the container

```bash
docker compose up -d --build
```

Open **http://your-server:3000**, register an account, and start adding downloads.

### 3. Path alignment (important)

The app container must see the same filesystem paths that qBittorrent reports. By default:

- qBittorrent downloads → `/downloads` inside the container
- Audiobook library → `/audiobooks` inside the container

Map these to the **same host folders** your qBittorrent and Audiobookshelf containers already use. If paths do not match, downloads will complete but the copy step will fail.

## Usage

### From AudiobookBay

1. Open a torrent page on AudiobookBay
2. Copy the **info hash** (40-character hex string)
3. In Audiobooker, paste the hash into **Magnet link or info hash**
4. Optionally add a **Display name** (helpful before metadata arrives from the swarm)
5. Click **Add download**

You can also paste a full `magnet:?xt=urn:btih:...` link if you already have one.

### Tracking downloads

The dashboard shows status, progress, speed, and ETA for each of your torrents. Progress refreshes automatically. When a download finishes, status changes to `copied` and the destination path is shown.

### Removing a download

Use the trash icon on a row to remove it from qBittorrent and the app. This does not delete files already copied to your library.

## Unraid notes

If you already run qBittorrent on Unraid:

- Point `QBITTORRENT_URL` at your existing container (hostname or IP on a shared Docker network)
- Mount your qBittorrent download folder and audiobook library into the Audiobooker container
- You do not need the optional qBittorrent service in `docker-compose.yml`

Example volume mapping on Unraid:

```
/mnt/user/data/torrents  →  /downloads   (read-only in Audiobooker)
/mnt/user/data/audiobooks  →  /audiobooks
```

## Local development

Requirements: Node.js 22+, pnpm

```bash
pnpm install
cp .env.example .env
# Edit .env — for local dev, DATABASE_PATH can be omitted (defaults to data/audiobooker.db)
pnpm dev
```

Visit **http://localhost:3000**.

## Configuration reference

See [`.env.example`](.env.example) for all options. Notable settings:

- `ALLOW_REGISTRATION=false` — disable new signups after initial setup
- `ADMIN_USERNAME` / `ADMIN_PASSWORD` — seed an admin account on first startup
- `SYNC_INTERVAL_MS` — how often to poll qBittorrent (default: 10 seconds)

## Tech stack

- [Next.js](https://nextjs.org/) (App Router)
- [SQLite](https://www.sqlite.org/) via better-sqlite3
- [qBittorrent Web API](https://github.com/qbittorrent/qBittorrent/wiki/WebUI-API-(qBittorrent-4.1))
- Docker with standalone Next.js output
