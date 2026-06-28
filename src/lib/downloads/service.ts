import { and, desc, eq, inArray, not } from "drizzle-orm";

import { copyCompletedDownload } from "@/lib/files/copy";
import { getDb } from "@/lib/db";
import { downloads } from "@/lib/db/schema";
import {
  getQBittorrentClient,
  mapTorrentState,
} from "@/lib/qbittorrent/client";

const ACTIVE_STATUSES = ["pending", "downloading", "completed", "copying"] as const;

export async function syncDownloads() {
  const db = getDb();
  const client = getQBittorrentClient();

  const activeDownloads = await db
    .select()
    .from(downloads)
    .where(inArray(downloads.status, [...ACTIVE_STATUSES]));

  if (activeDownloads.length === 0) {
    return { synced: 0, copied: 0 };
  }

  let torrents;
  try {
    torrents = await client.getTorrents();
  } catch (error) {
    console.error("Failed to sync with qBittorrent:", error);
    return { synced: 0, copied: 0, error: String(error) };
  }

  const torrentByHash = new Map(
    torrents.map((torrent) => [torrent.hash.toLowerCase(), torrent]),
  );

  let synced = 0;
  let copied = 0;

  for (const download of activeDownloads) {
    const torrent = torrentByHash.get(download.infoHash.toLowerCase());

    if (!torrent) {
      if (download.status === "pending") {
        continue;
      }
      await db
        .update(downloads)
        .set({
          status: "error",
          errorMessage: "Torrent not found in qBittorrent",
        })
        .where(eq(downloads.id, download.id));
      synced += 1;
      continue;
    }

    const status = mapTorrentState(torrent.state, torrent.progress);
    const updates: Partial<typeof downloads.$inferInsert> = {
      name: torrent.name,
      progress: torrent.progress,
      downloadSpeed: torrent.dlspeed,
      eta: torrent.eta,
      savePath: torrent.save_path,
      contentPath: torrent.content_path,
      status,
      errorMessage: status === "error" ? `qBittorrent state: ${torrent.state}` : null,
    };

    if (status === "completed" && !download.completedAt) {
      updates.completedAt = new Date();
    }

    await db.update(downloads).set(updates).where(eq(downloads.id, download.id));
    synced += 1;

    if (
      status === "completed" &&
      download.status !== "copied" &&
      download.status !== "copying"
    ) {
      await db
        .update(downloads)
        .set({ status: "copying" })
        .where(eq(downloads.id, download.id));

      try {
        const sourcePath = torrent.content_path || torrent.save_path;
        const destinationPath = await copyCompletedDownload(
          sourcePath,
          torrent.name,
        );

        await db
          .update(downloads)
          .set({
            status: "copied",
            destinationPath,
            copiedAt: new Date(),
            errorMessage: null,
          })
          .where(eq(downloads.id, download.id));
        copied += 1;
      } catch (error) {
        await db
          .update(downloads)
          .set({
            status: "error",
            errorMessage:
              error instanceof Error ? error.message : "Failed to copy files",
          })
          .where(eq(downloads.id, download.id));
      }
    }
  }

  return { synced, copied };
}

export async function listDownloadsForUser(userId: number) {
  const db = getDb();
  return db
    .select()
    .from(downloads)
    .where(eq(downloads.userId, userId))
    .orderBy(desc(downloads.createdAt));
}

export async function createDownload(
  userId: number,
  magnetUri: string,
  infoHash: string,
  name: string | null,
) {
  const db = getDb();
  const client = getQBittorrentClient();

  const existing = await db
    .select()
    .from(downloads)
    .where(
      and(
        eq(downloads.userId, userId),
        eq(downloads.infoHash, infoHash),
        not(eq(downloads.status, "error")),
      ),
    )
    .limit(1);

  if (existing.length > 0) {
    throw new Error("This torrent is already in your downloads");
  }

  await client.addMagnet(magnetUri, "audiobooks");

  const [download] = await db
    .insert(downloads)
    .values({
      userId,
      magnetUri,
      infoHash: infoHash.toLowerCase(),
      name,
      status: "pending",
    })
    .returning();

  return download;
}

export async function deleteDownloadForUser(userId: number, downloadId: number) {
  const db = getDb();
  const client = getQBittorrentClient();

  const [download] = await db
    .select()
    .from(downloads)
    .where(and(eq(downloads.id, downloadId), eq(downloads.userId, userId)))
    .limit(1);

  if (!download) {
    return null;
  }

  try {
    await client.deleteTorrent(download.infoHash, false);
  } catch (error) {
    console.error("Failed to remove torrent from qBittorrent:", error);
  }

  await db.delete(downloads).where(eq(downloads.id, download.id));
  return download;
}
