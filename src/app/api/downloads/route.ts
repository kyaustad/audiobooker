import { NextResponse } from "next/server";

import { requireUser } from "@/lib/auth/session";
import {
  createDownload,
  listDownloadsForUser,
} from "@/lib/downloads/service";
import { parseDownloadInput } from "@/lib/magnet";

export async function GET() {
  const session = await requireUser();
  if (!session) {
    return NextResponse.json({ error: "Unauthorized" }, { status: 401 });
  }

  const downloads = await listDownloadsForUser(session.userId);
  return NextResponse.json({ downloads });
}

export async function POST(request: Request) {
  const session = await requireUser();
  if (!session) {
    return NextResponse.json({ error: "Unauthorized" }, { status: 401 });
  }

  const body = await request.json();
  const input = String(body.input ?? body.magnetUri ?? "").trim();
  const name = body.name ? String(body.name).trim() : null;

  const parsed = parseDownloadInput(input, name);
  if (!parsed) {
    return NextResponse.json(
      { error: "Invalid magnet link or info hash" },
      { status: 400 },
    );
  }

  try {
    const download = await createDownload(
      session.userId,
      parsed.magnetUri,
      parsed.infoHash,
      parsed.name,
    );
    return NextResponse.json({ download }, { status: 201 });
  } catch (error) {
    return NextResponse.json(
      { error: error instanceof Error ? error.message : "Failed to add download" },
      { status: 500 },
    );
  }
}
