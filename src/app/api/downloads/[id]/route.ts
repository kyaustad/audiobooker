import { NextResponse } from "next/server";

import { requireUser } from "@/lib/auth/session";
import { deleteDownloadForUser } from "@/lib/downloads/service";

export async function DELETE(
  _request: Request,
  { params }: { params: Promise<{ id: string }> },
) {
  const session = await requireUser();
  if (!session) {
    return NextResponse.json({ error: "Unauthorized" }, { status: 401 });
  }

  const { id } = await params;
  const downloadId = Number(id);
  if (!Number.isFinite(downloadId)) {
    return NextResponse.json({ error: "Invalid download id" }, { status: 400 });
  }

  const deleted = await deleteDownloadForUser(session.userId, downloadId);
  if (!deleted) {
    return NextResponse.json({ error: "Download not found" }, { status: 404 });
  }

  return NextResponse.json({ ok: true });
}
