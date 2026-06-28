function required(name: string): string {
  const value = process.env[name];
  if (!value) {
    throw new Error(`Missing required environment variable: ${name}`);
  }
  return value;
}

export const env = {
  get sessionSecret() {
    return process.env.SESSION_SECRET ?? "dev-only-change-me-in-production";
  },
  get databasePath() {
    return process.env.DATABASE_PATH ?? "data/audiobooker.db";
  },
  get qbittorrentUrl() {
    return (process.env.QBITTORRENT_URL ?? "http://localhost:8080").replace(
      /\/$/,
      "",
    );
  },
  get qbittorrentUsername() {
    return process.env.QBITTORRENT_USERNAME ?? "admin";
  },
  get qbittorrentPassword() {
    return process.env.QBITTORRENT_PASSWORD ?? "adminadmin";
  },
  get qbittorrentDownloadPath() {
    return process.env.QBITTORRENT_DOWNLOAD_PATH ?? "/downloads";
  },
  get audiobookDestPath() {
    return process.env.AUDIOBOOK_DEST_PATH ?? "/audiobooks";
  },
  get allowRegistration() {
    return process.env.ALLOW_REGISTRATION !== "false";
  },
  get syncIntervalMs() {
    return Number(process.env.SYNC_INTERVAL_MS ?? "10000");
  },
  get adminUsername() {
    return process.env.ADMIN_USERNAME;
  },
  get adminPassword() {
    return process.env.ADMIN_PASSWORD;
  },
};

export function assertProductionSecrets() {
  if (process.env.NODE_ENV === "production") {
    required("SESSION_SECRET");
  }
}
