import { env } from "@/lib/env";

let syncTimer: NodeJS.Timeout | null = null;
let syncRunning = false;

export async function register() {
  if (process.env.NEXT_RUNTIME !== "nodejs") {
    return;
  }

  if (syncTimer) {
    return;
  }

  const { getDb } = await import("@/lib/db");
  const { seedAdminIfNeeded } = await import("@/lib/auth/users");
  const { syncDownloads } = await import("@/lib/downloads/service");
  const { assertProductionSecrets } = await import("@/lib/env");

  assertProductionSecrets();
  getDb();
  await seedAdminIfNeeded();

  const runSync = async () => {
    if (syncRunning) {
      return;
    }
    syncRunning = true;
    try {
      await syncDownloads();
    } catch (error) {
      console.error("Background sync failed:", error);
    } finally {
      syncRunning = false;
    }
  };

  await runSync();
  syncTimer = setInterval(runSync, env.syncIntervalMs);
}
