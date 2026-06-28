import { NextResponse } from "next/server";

import { getSession } from "@/lib/auth/session";
import { canRegister, createUser, findUserByUsername } from "@/lib/auth/users";

export async function GET() {
  return NextResponse.json({ allowed: await canRegister() });
}

export async function POST(request: Request) {
  const allowed = await canRegister();
  if (!allowed) {
    return NextResponse.json({ error: "Registration is disabled" }, { status: 403 });
  }

  const body = await request.json();
  const username = String(body.username ?? "").trim();
  const password = String(body.password ?? "");

  if (!username || !password) {
    return NextResponse.json(
      { error: "Username and password are required" },
      { status: 400 },
    );
  }

  if (username.length < 3) {
    return NextResponse.json(
      { error: "Username must be at least 3 characters" },
      { status: 400 },
    );
  }

  if (password.length < 8) {
    return NextResponse.json(
      { error: "Password must be at least 8 characters" },
      { status: 400 },
    );
  }

  const existing = await findUserByUsername(username);
  if (existing) {
    return NextResponse.json({ error: "Username already exists" }, { status: 409 });
  }

  const user = await createUser(username, password);

  const session = await getSession();
  session.userId = user.id;
  session.username = user.username;
  session.isLoggedIn = true;
  await session.save();

  return NextResponse.json({
    user: {
      id: user.id,
      username: user.username,
    },
  });
}
