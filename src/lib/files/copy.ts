import { cp, mkdir, stat } from "fs/promises";
import { basename, join } from "path";

import { env } from "@/lib/env";

async function pathExists(path: string) {
  try {
    await stat(path);
    return true;
  } catch {
    return false;
  }
}

function sanitizeName(name: string) {
  return name.replace(/[<>:"/\\|?*\x00-\x1f]/g, "_").trim() || "audiobook";
}

export async function copyCompletedDownload(
  sourcePath: string,
  torrentName: string,
) {
  const exists = await pathExists(sourcePath);
  if (!exists) {
    throw new Error(`Source path does not exist: ${sourcePath}`);
  }

  const sourceStats = await stat(sourcePath);
  const folderName = sanitizeName(
    sourceStats.isDirectory() ? torrentName : basename(sourcePath),
  );
  const destinationRoot = env.audiobookDestPath;
  const destinationPath = join(destinationRoot, folderName);

  await mkdir(destinationRoot, { recursive: true });

  if (await pathExists(destinationPath)) {
    throw new Error(`Destination already exists: ${destinationPath}`);
  }

  await cp(sourcePath, destinationPath, { recursive: true });
  return destinationPath;
}
