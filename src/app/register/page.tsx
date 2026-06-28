import Link from "next/link";
import { Suspense } from "react";

import { AuthForm } from "@/components/auth-form";
import { canRegister } from "@/lib/auth/users";

export const dynamic = "force-dynamic";

export default async function RegisterPage() {
  const allowed = await canRegister();

  if (!allowed) {
    return (
      <div className="flex min-h-full flex-1 items-center justify-center p-6">
        <div className="max-w-md space-y-3 text-center">
          <h1 className="text-2xl font-semibold">Registration disabled</h1>
          <p className="text-sm text-muted-foreground">
            New accounts are not being accepted. Contact an administrator if you need
            access.
          </p>
          <Link href="/login" className="text-sm text-primary hover:underline">
            Back to sign in
          </Link>
        </div>
      </div>
    );
  }

  return (
    <Suspense>
      <AuthForm mode="register" />
    </Suspense>
  );
}
