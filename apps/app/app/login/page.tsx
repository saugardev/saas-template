import type { Metadata } from "next";
import Link from "next/link";
import { loginAction } from "@/app/actions";
import { AuthForm } from "@/components/auth-form";
import { AuthShell } from "@/components/auth-shell";
import { api, safeReturnTo } from "@/lib/api";

export const metadata: Metadata = { title: "Sign in" };

type Provider = { slug: string; label: string };

export default async function LoginPage({
  searchParams,
}: {
  searchParams: Promise<Record<string, string | string[] | undefined>>;
}) {
  const params = await searchParams;
  const returnTo = safeReturnTo(
    typeof params.return_to === "string" ? params.return_to : undefined,
  );
  const providers = await api<{ providers: Provider[] }>(
    "/api/v1/auth/providers",
  ).catch(() => ({ providers: [] }));
  const apiUrl = process.env.API_PUBLIC_URL ?? "http://localhost:4000";

  return (
    <AuthShell
      title="Welcome back"
      description="Sign in to manage projects and agent connections."
    >
      {params.reset === "1" ? (
        <p
          role="status"
          className="mb-5 rounded-lg bg-success/10 px-4 py-3 text-sm text-success"
        >
          Password updated. Sign in with your new password.
        </p>
      ) : null}
      {providers.providers.length ? (
        <div className="mb-6 space-y-3">
          {providers.providers.map((provider) => (
            <a
              key={provider.slug}
              className="flex min-h-11 w-full items-center justify-center rounded-lg border border-border bg-card px-4 py-2.5 text-sm font-semibold transition-colors hover:bg-muted focus-visible:ring-2 focus-visible:ring-ring"
              href={`${apiUrl}/api/v1/auth/oidc/${encodeURIComponent(provider.slug)}/start?return_to=${encodeURIComponent(returnTo)}`}
            >
              Continue with {provider.label}
            </a>
          ))}
          <div className="flex items-center gap-3 py-1 text-xs text-muted-foreground">
            <span className="h-px flex-1 bg-border" />
            or use email
            <span className="h-px flex-1 bg-border" />
          </div>
        </div>
      ) : null}
      <AuthForm mode="login" action={loginAction} returnTo={returnTo} />
      <p className="mt-8 text-center text-xs leading-5 text-muted-foreground">
        Looking for setup instructions?{" "}
        <Link className="link" href={process.env.DOCS_URL ?? "http://localhost:3003"}>
          Read the docs
        </Link>
        .
      </p>
    </AuthShell>
  );
}

