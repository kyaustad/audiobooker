import { NextResponse } from "next/server";

import { getSession } from "@/lib/auth/session";
import { findUserById } from "@/lib/auth/users";

export async function GET() {
  const session = await getSession();
  if (!session.isLoggedIn) {
    return NextResponse.json({ user: null });
  }

  const user = await findUserById(session.userId);
  if (!user) {
    session.destroy();
    return NextResponse.json({ user: null });
  }

  return NextResponse.json({
    user: {
      id: user.id,
      username: user.username,
    },
  });
}
