import { env } from "@/lib/env";

export interface QBittorrentTorrent {
  hash: string;
  name: string;
  progress: number;
  dlspeed: number;
  eta: number;
  state: string;
  save_path: string;
  content_path: string;
}

class QBittorrentClient {
  private baseUrl: string;
  private username: string;
  private password: string;
  private cookie: string | null = null;

  constructor() {
    this.baseUrl = env.qbittorrentUrl;
    this.username = env.qbittorrentUsername;
    this.password = env.qbittorrentPassword;
  }

  private async ensureLoggedIn() {
    if (this.cookie) {
      return;
    }
    await this.login();
  }

  private getHeaders(extra?: HeadersInit): HeadersInit {
    return {
      ...(this.cookie ? { Cookie: this.cookie } : {}),
      ...extra,
    };
  }

  async login() {
    const response = await fetch(`${this.baseUrl}/api/v2/auth/login`, {
      method: "POST",
      headers: { "Content-Type": "application/x-www-form-urlencoded" },
      body: new URLSearchParams({
        username: this.username,
        password: this.password,
      }),
      cache: "no-store",
    });

    if (!response.ok) {
      throw new Error(`qBittorrent login failed (${response.status})`);
    }

    const body = await response.text();
    if (body === "Fails.") {
      throw new Error("qBittorrent login failed: invalid credentials");
    }

    const setCookie = response.headers.getSetCookie?.() ?? [];
    const sid = setCookie.find((cookie) => cookie.startsWith("SID="));
    if (!sid) {
      throw new Error("qBittorrent login failed: missing session cookie");
    }

    this.cookie = sid.split(";")[0];
  }

  async addMagnet(magnetUri: string, category?: string): Promise<void> {
    await this.ensureLoggedIn();

    const params = new URLSearchParams({ urls: magnetUri });
    if (category) {
      params.set("category", category);
    }

    const response = await fetch(`${this.baseUrl}/api/v2/torrents/add`, {
      method: "POST",
      headers: {
        ...this.getHeaders(),
        "Content-Type": "application/x-www-form-urlencoded",
      },
      body: params,
      cache: "no-store",
    });

    if (response.status === 403) {
      this.cookie = null;
      await this.login();
      return this.addMagnet(magnetUri, category);
    }

    if (!response.ok) {
      throw new Error(`Failed to add magnet (${response.status})`);
    }
  }

  async getTorrents(): Promise<QBittorrentTorrent[]> {
    await this.ensureLoggedIn();

    const response = await fetch(`${this.baseUrl}/api/v2/torrents/info`, {
      headers: this.getHeaders(),
      cache: "no-store",
    });

    if (response.status === 403) {
      this.cookie = null;
      await this.login();
      return this.getTorrents();
    }

    if (!response.ok) {
      throw new Error(`Failed to fetch torrents (${response.status})`);
    }

    return response.json();
  }

  async getTorrentByHash(hash: string): Promise<QBittorrentTorrent | null> {
    const torrents = await this.getTorrents();
    const normalized = hash.toLowerCase();
    return (
      torrents.find((torrent) => torrent.hash.toLowerCase() === normalized) ??
      null
    );
  }

  async deleteTorrent(hash: string, deleteFiles = false): Promise<void> {
    await this.ensureLoggedIn();

    const params = new URLSearchParams({
      hashes: hash,
      deleteFiles: deleteFiles ? "true" : "false",
    });

    const response = await fetch(`${this.baseUrl}/api/v2/torrents/delete`, {
      method: "POST",
      headers: {
        ...this.getHeaders(),
        "Content-Type": "application/x-www-form-urlencoded",
      },
      body: params,
      cache: "no-store",
    });

    if (response.status === 403) {
      this.cookie = null;
      await this.login();
      return this.deleteTorrent(hash, deleteFiles);
    }

    if (!response.ok) {
      throw new Error(`Failed to delete torrent (${response.status})`);
    }
  }
}

let client: QBittorrentClient | null = null;

export function getQBittorrentClient() {
  if (!client) {
    client = new QBittorrentClient();
  }
  return client;
}

export function mapTorrentState(state: string, progress: number) {
  if (progress >= 1) {
    return "completed" as const;
  }

  switch (state) {
    case "error":
    case "missingFiles":
      return "error" as const;
    case "queuedDL":
    case "checkingDL":
    case "stalledDL":
    case "downloading":
    case "metaDL":
    case "forcedDL":
      return "downloading" as const;
    default:
      return "pending" as const;
  }
}
