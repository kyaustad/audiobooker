import { count, eq } from "drizzle-orm";

import { getDb } from "@/lib/db";
import { users } from "@/lib/db/schema";
import { hashPassword, verifyPassword } from "@/lib/auth/password";
import { env } from "@/lib/env";

export async function createUser(username: string, password: string) {
  const db = getDb();
  const passwordHash = await hashPassword(password);
  const [user] = await db
    .insert(users)
    .values({ username, passwordHash })
    .returning();
  return user;
}

export async function findUserByUsername(username: string) {
  const db = getDb();
  const [user] = await db
    .select()
    .from(users)
    .where(eq(users.username, username))
    .limit(1);
  return user ?? null;
}

export async function findUserById(id: number) {
  const db = getDb();
  const [user] = await db.select().from(users).where(eq(users.id, id)).limit(1);
  return user ?? null;
}

export async function authenticateUser(username: string, password: string) {
  const user = await findUserByUsername(username);
  if (!user) {
    return null;
  }
  const valid = await verifyPassword(password, user.passwordHash);
  return valid ? user : null;
}

export async function getUserCount() {
  const db = getDb();
  const [result] = await db.select({ value: count() }).from(users);
  return result?.value ?? 0;
}

export async function seedAdminIfNeeded() {
  const adminUsername = env.adminUsername;
  const adminPassword = env.adminPassword;
  if (!adminUsername || !adminPassword) {
    return;
  }

  const existing = await findUserByUsername(adminUsername);
  if (existing) {
    return;
  }

  await createUser(adminUsername, adminPassword);
}

export async function canRegister() {
  const userCount = await getUserCount();
  if (userCount === 0) {
    return true;
  }
  return env.allowRegistration;
}
