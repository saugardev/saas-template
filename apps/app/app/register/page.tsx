import type { Metadata } from "next";
import { registerAction } from "@/app/actions";
import { AuthForm } from "@/components/auth-form";
import { AuthShell } from "@/components/auth-shell";

export const metadata: Metadata = { title: "Create account" };

export default function RegisterPage() {
  return (
    <AuthShell
      title="Create your account"
      description="Start with a private workspace and a default project."
    >
      <AuthForm mode="register" action={registerAction} />
    </AuthShell>
  );
}

