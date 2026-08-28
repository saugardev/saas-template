import type { Metadata } from "next";
import { resetPasswordAction } from "@/app/actions";
import { AuthShell } from "@/components/auth-shell";
import { SimpleForm } from "@/components/simple-form";

export const metadata: Metadata = { title: "Choose password" };

export default async function ResetPasswordPage({
  searchParams,
}: {
  searchParams: Promise<{ token?: string }>;
}) {
  const { token = "" } = await searchParams;
  return (
    <AuthShell
      title="Choose a new password"
      description="This will sign out your existing sessions."
    >
      <SimpleForm action={resetPasswordAction} submitLabel="Update password">
        <input type="hidden" name="token" value={token} />
        <div>
          <label className="mb-1.5 block text-sm font-medium" htmlFor="password">
            New password
          </label>
          <input
            className="field"
            id="password"
            name="password"
            type="password"
            autoComplete="new-password"
            minLength={8}
            required
          />
          <p className="mt-1.5 text-xs text-muted-foreground">
            Use at least 8 characters.
          </p>
        </div>
      </SimpleForm>
    </AuthShell>
  );
}

