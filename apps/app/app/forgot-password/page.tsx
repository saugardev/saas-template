import type { Metadata } from "next";
import Link from "next/link";
import { forgotPasswordAction } from "@/app/actions";
import { AuthShell } from "@/components/auth-shell";
import { SimpleForm } from "@/components/simple-form";

export const metadata: Metadata = { title: "Reset password" };

export default function ForgotPasswordPage() {
  return (
    <AuthShell
      title="Reset your password"
      description="We’ll send a reset link if the account exists."
    >
      <SimpleForm action={forgotPasswordAction} submitLabel="Send reset link">
        <div>
          <label className="mb-1.5 block text-sm font-medium" htmlFor="email">
            Email
          </label>
          <input
            className="field"
            id="email"
            name="email"
            type="email"
            autoComplete="email"
            spellCheck={false}
            required
          />
        </div>
      </SimpleForm>
      <Link className="link mt-6 inline-block text-sm" href="/login">
        Back to sign in
      </Link>
    </AuthShell>
  );
}

