import type { Metadata } from "next";
import { redirect } from "next/navigation";
import { consentAction } from "@/app/actions";
import { AuthShell } from "@/components/auth-shell";
import { ApiError, api } from "@/lib/api";

export const metadata: Metadata = { title: "Authorize connection" };

type Consent = {
  request_id: string;
  client_id: string;
  client_name: string;
  client_uri?: string;
  registration_type: "cimd" | "dynamic" | "static";
  redirect_uri: string;
  scopes: string[];
  resource: string;
  expires_at: string;
  projects: Array<{
    workspace_id: string;
    workspace_name: string;
    project_id: string;
    project_name: string;
  }>;
};

export default async function ConsentPage({
  searchParams,
}: {
  searchParams: Promise<{ request_id?: string }>;
}) {
  const { request_id: requestId } = await searchParams;
  if (!requestId) redirect("/dashboard");
  let consent: Consent;
  try {
    consent = await api<Consent>(
      `/api/v1/oauth/authorization-requests/${encodeURIComponent(requestId)}`,
    );
  } catch (error) {
    if (error instanceof ApiError && error.status === 401) {
      redirect(
        `/login?return_to=${encodeURIComponent(`/oauth/consent?request_id=${requestId}`)}`,
      );
    }
    throw error;
  }
  const defaultProject = consent.projects[0];
  const clientHost = urlHost(consent.client_id);
  const redirectHost = urlHost(consent.redirect_uri);

  return (
    <AuthShell
      title="Authorize this connection"
      description={`${consent.client_name} is requesting access to one project.`}
    >
      <div className="space-y-6">
        <div className="rounded-lg border border-border bg-card p-4">
          <p className="text-sm font-semibold">Connection identity</p>
          <dl className="mt-3 grid gap-2 text-sm">
            <div className="flex items-baseline justify-between gap-4">
              <dt className="text-muted-foreground">Client</dt>
              <dd className="text-right font-medium">
                {clientHost ?? `${consent.registration_type} client`}
              </dd>
            </div>
            <div className="flex items-baseline justify-between gap-4">
              <dt className="text-muted-foreground">Redirects to</dt>
              <dd className="text-right font-mono text-xs">{redirectHost ?? "Unknown host"}</dd>
            </div>
          </dl>
          {redirectHost === "localhost" || redirectHost === "127.0.0.1" || redirectHost === "[::1]" ? (
            <p className="mt-3 rounded-lg bg-muted px-3 py-2 text-xs leading-5 text-muted-foreground">
              This native client will return the authorization code to an app running on this device.
            </p>
          ) : null}
        </div>
        <div className="rounded-lg border border-border bg-card p-4">
          <p className="text-sm font-semibold">Requested permissions</p>
          <ul className="mt-3 space-y-2 text-sm text-muted-foreground">
            {consent.scopes.map((scope) => (
              <li key={scope} className="flex items-center justify-between gap-4">
                <code className="font-mono text-xs text-foreground">{scope}</code>
                <span>{scopeDescription(scope)}</span>
              </li>
            ))}
          </ul>
        </div>
        <form action={consentAction} className="space-y-5">
          <input type="hidden" name="request_id" value={consent.request_id} />
          <fieldset>
            <legend className="mb-3 text-sm font-semibold">Share one project</legend>
            <div className="space-y-2">
              {consent.projects.map((project, index) => (
                <label
                  key={project.project_id}
                  className="flex min-h-12 cursor-pointer items-center gap-3 rounded-lg border border-border px-3 py-2.5 text-sm transition-colors hover:bg-muted has-[:checked]:border-foreground has-[:checked]:bg-muted"
                >
                  <input
                    type="radio"
                    name="selection"
                    value={`${project.workspace_id}:${project.project_id}`}
                    defaultChecked={index === 0}
                    required
                  />
                  <span>
                    <span className="block font-medium">{project.project_name}</span>
                    <span className="block text-xs text-muted-foreground">
                      {project.workspace_name}
                    </span>
                  </span>
                </label>
              ))}
            </div>
          </fieldset>
          {!defaultProject ? (
            <p role="alert" className="text-sm text-destructive">
              Create a project before approving this connection.
            </p>
          ) : null}
          <div className="grid gap-3 sm:grid-cols-2">
            <button
              type="submit"
              name="decision"
              value="deny"
              className="min-h-11 rounded-lg border border-border px-4 text-sm font-semibold hover:bg-muted focus-visible:ring-2 focus-visible:ring-ring"
            >
              Deny
            </button>
            <button
              type="submit"
              name="decision"
              value="approve"
              disabled={!defaultProject}
              className="min-h-11 rounded-lg bg-primary px-4 text-sm font-semibold text-primary-foreground hover:bg-primary/90 focus-visible:ring-2 focus-visible:ring-ring disabled:opacity-50"
            >
              Allow access
            </button>
          </div>
        </form>
        <p className="text-xs leading-5 text-muted-foreground">
          Access is limited to the selected project and can be revoked by
          disconnecting the client. The client never receives your application
          password or browser session.
        </p>
      </div>
    </AuthShell>
  );
}

function urlHost(value: string) {
  try {
    return new URL(value).hostname;
  } catch {
    return null;
  }
}

function scopeDescription(scope: string) {
  const descriptions: Record<string, string> = {
    openid: "Identify your account",
    profile: "Read your display name",
    email: "Read your email address and verification status",
    offline_access: "Stay connected",
    "project:read": "Read selected project context",
  };
  return descriptions[scope] ?? "Use this permission";
}
