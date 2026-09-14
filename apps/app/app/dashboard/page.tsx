import type { Metadata } from "next";
import Link from "next/link";
import { redirect } from "next/navigation";
import {
  createApiKeyAction,
  createProjectAction,
  createWorkspaceAction,
  logoutAction,
  revokeApiKeyAction,
  selectContextAction,
} from "@/app/actions";
import { SimpleForm } from "@/components/simple-form";
import { ConfirmSubmitButton } from "@/components/confirm-submit-button";
import { ApiError, api } from "@/lib/api";

export const metadata: Metadata = { title: "Dashboard" };

type Me = {
  user_id: string;
  email: string;
  name: string;
  workspace_id: string;
  project_id: string;
  role: string;
};

type Workspace = {
  id: string;
  name: string;
  slug: string;
  role: string;
  projects: Array<{ id: string; name: string; slug: string }>;
};

type ApiKey = {
  id: string;
  name: string;
  prefix: string;
  scopes: string[];
  created_at: string;
};

export default async function DashboardPage() {
  let me: Me;
  let workspaces: Workspace[];
  try {
    [me, workspaces] = await Promise.all([
      api<Me>("/api/v1/me"),
      api<Workspace[]>("/api/v1/workspaces"),
    ]);
  } catch (error) {
    if (error instanceof ApiError && error.status === 401) redirect("/login");
    throw error;
  }
  const activeWorkspace = workspaces.find((workspace) => workspace.id === me.workspace_id);
  const activeProject = activeWorkspace?.projects.find(
    (project) => project.id === me.project_id,
  );
  const canManageActive =
    activeWorkspace?.role === "owner" || activeWorkspace?.role === "admin";
  const keys = await api<ApiKey[]>(
    `/api/v1/projects/${encodeURIComponent(me.project_id)}/api-keys`,
  );

  return (
    <main className="min-h-screen">
      <header className="border-b border-border">
        <div className="mx-auto flex min-h-16 max-w-7xl items-center justify-between gap-4 px-4 sm:px-6 lg:px-8">
          <Link
            href="/dashboard"
            className="inline-flex min-h-10 items-center rounded-sm text-sm font-semibold focus-visible:ring-2 focus-visible:ring-ring"
          >
            Agent SaaS Starter
          </Link>
          <div className="flex items-center gap-3">
            <span className="hidden text-sm text-muted-foreground sm:inline">
              {me.email}
            </span>
            <form action={logoutAction}>
              <button
                type="submit"
                className="min-h-10 rounded-lg border border-border px-3 text-sm font-medium hover:bg-muted focus-visible:ring-2 focus-visible:ring-ring"
              >
                Sign out
              </button>
            </form>
          </div>
        </div>
      </header>
      <div className="mx-auto max-w-7xl px-4 py-10 sm:px-6 lg:px-8">
        <div className="mb-10 max-w-2xl space-y-2">
          <p className="font-mono text-xs uppercase tracking-[0.16em] text-muted-foreground">
            {activeWorkspace?.name ?? "Workspace"} / {activeProject?.name ?? "Project"}
          </p>
          <h1 className="text-3xl font-semibold tracking-tight">
            Good to see you, {me.name}.
          </h1>
          <p className="leading-6 text-muted-foreground">
            This dashboard is intentionally small: identity, tenant boundaries,
            credentials, and agent access.
          </p>
        </div>
        <div className="grid gap-6 lg:grid-cols-3">
          <section className="rounded-xl border border-border bg-card p-5 lg:col-span-2">
            <div className="mb-5">
              <h2 className="font-semibold">Projects</h2>
              <p className="mt-1 text-sm text-muted-foreground">
                Each agent grant is bound to exactly one project.
              </p>
            </div>
            <div className="space-y-4">
              {workspaces.length ? (
                workspaces.map((workspace) => (
                  <div key={workspace.id} className="rounded-lg bg-muted/60 p-4">
                    <div className="flex items-baseline justify-between gap-3">
                      <h3 className="text-sm font-semibold">{workspace.name}</h3>
                      <span className="font-mono text-xs text-muted-foreground">
                        {workspace.role}
                      </span>
                    </div>
                    {workspace.projects.length ? (
                      <ul className="mt-3 divide-y divide-border">
                        {workspace.projects.map((project) => (
                          <li
                            key={project.id}
                            className="flex min-h-11 items-center justify-between gap-4 py-2 text-sm"
                          >
                            <div>
                              <span className="font-medium">{project.name}</span>
                              <code className="ml-3 font-mono text-xs text-muted-foreground">
                                {project.slug}
                              </code>
                            </div>
                            {project.id === me.project_id ? (
                              <span className="rounded-full bg-background px-2 py-1 text-xs font-medium">
                                Active
                              </span>
                            ) : (
                              <form action={selectContextAction}>
                                <input type="hidden" name="workspace_id" value={workspace.id} />
                                <input type="hidden" name="project_id" value={project.id} />
                                <button
                                  type="submit"
                                  className="min-h-10 rounded-lg border border-border bg-background px-3 text-xs font-medium hover:bg-card focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                                >
                                  Switch
                                </button>
                              </form>
                            )}
                          </li>
                        ))}
                      </ul>
                    ) : (
                      <p className="mt-3 text-sm text-muted-foreground">
                        No projects yet. Create one below.
                      </p>
                    )}
                  </div>
                ))
              ) : (
                <div className="rounded-lg bg-muted p-6 text-center">
                  <p className="text-sm font-medium">No workspace yet</p>
                  <p className="mt-1 text-sm text-muted-foreground">
                    Create one to establish your first tenant boundary.
                  </p>
                </div>
              )}
            </div>
          </section>
          <section className="rounded-xl border border-border bg-card p-5">
            <h2 className="font-semibold">Create workspace</h2>
            <p className="mt-1 text-sm text-muted-foreground">
              Starts with a default project.
            </p>
            <div className="mt-5">
              <SimpleForm action={createWorkspaceAction} submitLabel="Create workspace">
                <label className="block text-sm font-medium" htmlFor="workspace-name">
                  Name
                </label>
                <input
                  className="field"
                  id="workspace-name"
                  name="name"
                  autoComplete="organization"
                  minLength={2}
                  required
                />
              </SimpleForm>
            </div>
          </section>
          {activeWorkspace && canManageActive ? (
            <section className="rounded-xl border border-border bg-card p-5">
              <h2 className="font-semibold">Create project</h2>
              <p className="mt-1 text-sm text-muted-foreground">
                Inside {activeWorkspace.name}.
              </p>
              <div className="mt-5">
                <SimpleForm action={createProjectAction} submitLabel="Create project">
                  <input type="hidden" name="workspace_id" value={activeWorkspace.id} />
                  <label className="block text-sm font-medium" htmlFor="project-name">
                    Name
                  </label>
                  <input className="field" id="project-name" name="name" minLength={2} required />
                </SimpleForm>
              </div>
            </section>
          ) : null}
          <section className="rounded-xl border border-border bg-card p-5 lg:col-span-2">
            <div className="mb-5">
              <h2 className="font-semibold">Project API keys</h2>
              <p className="mt-1 text-sm text-muted-foreground">
                Secrets are hashed and displayed only once.
              </p>
            </div>
            {keys.length ? (
              <ul className="mb-6 divide-y divide-border rounded-lg bg-muted/60 px-4">
                {keys.map((key) => (
                  <li key={key.id} className="flex min-h-14 items-center justify-between gap-4 py-2">
                    <div>
                      <p className="text-sm font-medium">{key.name}</p>
                      <code className="font-mono text-xs text-muted-foreground">{key.prefix}…</code>
                    </div>
                    <div className="flex items-center gap-3">
                      <span className="text-xs text-muted-foreground">
                        {key.scopes.join(", ")}
                      </span>
                      {canManageActive ? (
                        <form action={revokeApiKeyAction}>
                          <input type="hidden" name="api_key_id" value={key.id} />
                          <ConfirmSubmitButton
                            confirmMessage={`Revoke ${key.name}? Existing integrations using it will stop working.`}
                            className="min-h-10 rounded-lg px-2 text-xs font-medium text-destructive hover:bg-background focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring"
                          >
                            Revoke
                          </ConfirmSubmitButton>
                        </form>
                      ) : null}
                    </div>
                  </li>
                ))}
              </ul>
            ) : (
              <div className="mb-6 rounded-lg bg-muted p-5 text-sm text-muted-foreground">
                No API keys yet. Create one when a non-OAuth integration needs project access.
              </div>
            )}
            {canManageActive ? (
              <SimpleForm action={createApiKeyAction} submitLabel="Create API key">
                <input type="hidden" name="project_id" value={me.project_id} />
                <label className="block text-sm font-medium" htmlFor="api-key-name">
                  Key name
                </label>
                <input
                  className="field"
                  id="api-key-name"
                  name="name"
                  placeholder="Local development"
                  minLength={2}
                  required
                />
              </SimpleForm>
            ) : (
              <p className="text-sm text-muted-foreground">
                An owner or administrator can create and revoke project API keys.
              </p>
            )}
          </section>
        </div>
      </div>
    </main>
  );
}
