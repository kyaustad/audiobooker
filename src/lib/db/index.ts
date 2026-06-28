import Database from "better-sqlite3";
import { drizzle } from "drizzle-orm/better-sqlite3";
import { mkdirSync } from "fs";
import { dirname, isAbsolute, resolve } from "path";

import { env } from "@/lib/env";
import * as schema from "./schema";

let sqlite: Database.Database | null = null;
let db: ReturnType<typeof drizzle<typeof schema>> | null = null;

function resolveDatabasePath() {
  const configured = env.databasePath;
  return isAbsolute(configured) ? configured : resolve(process.cwd(), configured);
}

export function getDb() {
  if (!db) {
    const dbPath = resolveDatabasePath();
    mkdirSync(dirname(dbPath), { recursive: true });
    sqlite = new Database(dbPath);
    sqlite.pragma("journal_mode = WAL");
    sqlite.pragma("foreign_keys = ON");
    db = drizzle(sqlite, { schema });
    initSchema(sqlite);
  }
  return db;
}

function initSchema(database: Database.Database) {
  database.exec(`
    CREATE TABLE IF NOT EXISTS users (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      username TEXT NOT NULL UNIQUE,
      password_hash TEXT NOT NULL,
      created_at INTEGER NOT NULL DEFAULT (unixepoch())
    );

    CREATE TABLE IF NOT EXISTS downloads (
      id INTEGER PRIMARY KEY AUTOINCREMENT,
      user_id INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
      magnet_uri TEXT NOT NULL,
      name TEXT,
      info_hash TEXT NOT NULL,
      status TEXT NOT NULL DEFAULT 'pending',
      progress REAL NOT NULL DEFAULT 0,
      download_speed INTEGER NOT NULL DEFAULT 0,
      eta INTEGER NOT NULL DEFAULT 0,
      save_path TEXT,
      content_path TEXT,
      destination_path TEXT,
      error_message TEXT,
      created_at INTEGER NOT NULL DEFAULT (unixepoch()),
      completed_at INTEGER,
      copied_at INTEGER
    );

    CREATE INDEX IF NOT EXISTS idx_downloads_user_id ON downloads(user_id);
    CREATE INDEX IF NOT EXISTS idx_downloads_info_hash ON downloads(info_hash);
    CREATE INDEX IF NOT EXISTS idx_downloads_status ON downloads(status);
  `);
}

export { schema };
