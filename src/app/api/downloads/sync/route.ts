import { NextResponse } from "next/server";

import { requireUser } from "@/lib/auth/session";
import { listDownloadsForUser, syncDownloads } from "@/lib/downloads/service";

export async function GET() {
  const session = await requireUser();
  if (!session) {
    return NextResponse.json({ error: "Unauthorized" }, { status: 401 });
  }

  await syncDownloads();
  const downloads = await listDownloadsForUser(session.userId);
  return NextResponse.json({ downloads });
}
