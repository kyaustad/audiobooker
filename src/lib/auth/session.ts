import { sessionOptions, type SessionData } from "@/lib/auth/session-options";

export type { SessionData } from "@/lib/auth/session-options";
export { sessionOptions };

export async function getSession() {
  const { getIronSession } = await import("iron-session");
  const { cookies } = await import("next/headers");
  const cookieStore = await cookies();
  return getIronSession<SessionData>(cookieStore, sessionOptions);
}

export async function requireUser() {
  const session = await getSession();
  if (!session.isLoggedIn || !session.userId) {
    return null;
  }
  return session;
}
