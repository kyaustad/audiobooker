import { redirect } from "next/navigation";

import { Dashboard } from "@/components/dashboard";
import { getSession } from "@/lib/auth/session";

export const dynamic = "force-dynamic";

export default async function HomePage() {
  const session = await getSession();

  if (!session.isLoggedIn) {
    redirect("/login");
  }

  return <Dashboard username={session.username} />;
}
