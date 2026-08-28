import type { Metadata } from "next";
import Link from "next/link";
import { verifyEmailAction } from "@/app/actions";
import { AuthShell } from "@/components/auth-shell";
import { SimpleForm } from "@/components/simple-form";

export const metadata: Metadata = { title: "Verify email" };

export default async function VerifyEmailPage({
  searchParams,
}: {
  searchParams: Promise<{ token?: string }>;
}) {
  const { token = "" } = await searchParams;
  return (
    <AuthShell
      title="Verify your email"
      description="Confirm this address before sharing agent access."
    >
      <SimpleForm action={verifyEmailAction} submitLabel="Verify email">
        <input type="hidden" name="token" value={token} />
      </SimpleForm>
      <Link className="link mt-6 inline-block text-sm" href="/dashboard">
        Return to dashboard
      </Link>
    </AuthShell>
  );
}

