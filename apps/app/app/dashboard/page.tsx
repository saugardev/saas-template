import type { Metadata } from "next";
import Link from "next/link";
import { redirect } from "next/navigation";
import {
  BookOpen,
  Box,
  Building2,
  ExternalLink,
  Folder,
  KeyRound,
  LogOut,
  Plus,
  ShieldCheck,
} from "lucide-react";
import {
  createApiKeyAction,
  createProjectAction,
  createWorkspaceAction,
  logoutAction,
  revokeApiKeyAction,
  selectContextAction,
} from "@/app/actions";
import { ConfirmSubmitButton } from "@/components/confirm-submit-button";
import { SimpleForm } from "@/components/simple-form";
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

type View = "projects" | "workspaces" | "api-keys";

const viewDetails: Record<View, { title: string; description: string }> = {
  projects: {
    title: "Projects",
    description: "Choose the exact product boundary an integration or agent can access.",
  },
  workspaces: {
    title: "Workspaces",
    description: "Manage the tenant boundary shared by your team and projects.",
  },
  "api-keys": {
    title: "API keys",
    description: "Create and revoke credentials for the currently selected project.",
  },
};

export default async function DashboardPage({
  searchParams,
}: {
  searchParams: Promise<{ view?: string | string[]; create?: string | string[] }>;
}) {
  const params = await searchParams;
  const requestedView = params.view;
  const view: View =
    typeof requestedView === "string" && requestedView in viewDetails
      ? (requestedView as View)
      : "projects";
  const showCreate = params.create === "1";

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
  const activeProject = activeWorkspace?.projects.find((project) => project.id === me.project_id);
  const canManageActive = activeWorkspace?.role === "owner" || activeWorkspace?.role === "admin";
  const keys = view === "api-keys"
    ? await api<ApiKey[]>(`/api/v1/projects/${encodeURIComponent(me.project_id)}/api-keys`)
    : [];
  const docsUrl = process.env.DOCS_URL ?? "http://localhost:3003";
  const accessLabel = view === "api-keys"
    ? `${activeWorkspace?.role ?? me.role} access`
    : view === "projects"
      ? `${workspaces.reduce((total, workspace) => total + workspace.projects.length, 0)} visible projects`
      : `${workspaces.length} accessible workspaces`;

  return (
    <main className="min-h-screen bg-background p-2 sm:p-4 lg:p-10">
      <a className="skip-link" href="#dashboard-content">Skip to dashboard content</a>
      <div className="mx-auto grid min-h-[calc(100svh-1rem)] max-w-[1600px] overflow-hidden rounded-[22px] border border-border bg-card shadow-[0_1px_2px_rgb(0_0_0/0.04),0_18px_50px_rgb(0_0_0/0.06)] sm:min-h-[calc(100svh-2rem)] sm:rounded-[28px] md:grid-cols-[320px_minmax(0,1fr)] lg:min-h-[calc(100svh-5rem)] lg:grid-cols-[380px_minmax(0,1fr)]">
        <aside className="flex min-w-0 flex-col border-b border-border bg-card md:border-b-0 md:border-r">
          <div className="flex min-h-[96px] items-center border-b border-border px-5 lg:px-7">
            <Link className="sidebar-brand" href="/dashboard" aria-label="Agent SaaS dashboard">
              <BrandMark />
            </Link>
          </div>

          <div className="border-b border-border px-4 py-3 md:border-b-0 md:px-6 md:py-6">
            <div className="flex min-h-[68px] items-center gap-3 rounded-[14px] border border-border bg-subtle px-4 shadow-[0_1px_1px_rgb(0_0_0/0.02)]">
              <span className="grid size-9 shrink-0 place-items-center rounded-[10px] bg-card text-xs font-semibold shadow-[inset_0_0_0_1px_var(--border)]">
                {initials(activeWorkspace?.name ?? "Workspace")}
              </span>
              <span className="min-w-0">
                <small className="block text-[11px] text-muted-foreground">Selected workspace</small>
                <strong className="mt-0.5 block truncate text-[15px] font-medium">{activeWorkspace?.name ?? "No workspace"}</strong>
              </span>
            </div>
          </div>

          <div className="flex min-h-0 flex-1 gap-1 overflow-x-auto px-3 pb-3 md:flex-col md:overflow-y-auto md:px-5 md:pb-6">
            <p className="sidebar-label">Product</p>
            <SidebarLink href="/dashboard?view=projects" active={view === "projects"} icon={Folder}>Projects</SidebarLink>
            <SidebarLink href="/dashboard?view=workspaces" active={view === "workspaces"} icon={Building2}>Workspaces</SidebarLink>
            <SidebarLink href="/dashboard?view=api-keys" active={view === "api-keys"} icon={KeyRound}>API keys</SidebarLink>

            <p className="sidebar-label">Resources</p>
            <SidebarAnchor href={`${docsUrl}/docs/guides/mcp`} icon={Box}>MCP guide</SidebarAnchor>
            <SidebarAnchor href={`${docsUrl}/docs`} icon={BookOpen}>Documentation</SidebarAnchor>

            {workspaces.some((workspace) => workspace.projects.length) ? (
              <div className="hidden md:block">
                <p className="sidebar-label">Recents</p>
                {workspaces.flatMap((workspace) => workspace.projects.map((project) => ({ workspace, project }))).slice(0, 3).map(({ workspace, project }) => (
                  <form action={selectContextAction} key={project.id}>
                    <input type="hidden" name="workspace_id" value={workspace.id} />
                    <input type="hidden" name="project_id" value={project.id} />
                    <button className="recent-project" type="submit">
                      <span>{project.name}</span>{project.id === me.project_id ? <span className="active-dot" /> : null}
                    </button>
                  </form>
                ))}
              </div>
            ) : null}

            <div className="mt-auto hidden pt-8 md:block">
              <a className="agent-card" href={`${docsUrl}/docs/guides/mcp`}>
                <span className="agent-card-art" />
                <strong>Connect an agent <ExternalLink aria-hidden="true" /></strong>
                <span>Authorize tools with a narrow, project-scoped grant.</span>
              </a>
            </div>
          </div>

          <div className="hidden border-t border-border p-6 md:block">
            <div className="flex items-center gap-3 px-1">
              <span className="grid size-[60px] shrink-0 place-items-center rounded-full bg-subtle text-sm font-semibold">
                {initials(me.name)}
              </span>
              <span className="min-w-0 flex-1">
                <strong className="block truncate text-base font-medium">{me.name}</strong>
                <small className="mt-0.5 block truncate text-xs text-muted-foreground">{me.email}</small>
              </span>
              <form action={logoutAction}>
                <button className="icon-button" type="submit" aria-label="Sign out" title="Sign out">
                  <LogOut aria-hidden="true" />
                </button>
              </form>
            </div>
          </div>
        </aside>

        <section className="min-w-0 bg-[#fdfdfc]">
          <header className="flex min-h-[96px] items-center justify-between gap-4 border-b border-border px-5 sm:px-7 lg:px-10">
            <strong className="text-xs font-medium md:sr-only">{viewDetails[view].title}</strong>
            <div className="ml-auto flex items-center gap-2">
              <a className="button-secondary md:hidden" href={`${docsUrl}/docs`}>
                Help <ExternalLink aria-hidden="true" />
              </a>
              <form className="md:hidden" action={logoutAction}>
                <button className="icon-button" type="submit" aria-label="Sign out"><LogOut aria-hidden="true" /></button>
              </form>
            </div>
          </header>

          <div id="dashboard-content" className="mx-auto max-w-[1240px] px-5 py-8 sm:px-7 sm:py-10 lg:px-10 lg:py-14">
            <header className="mb-9 flex flex-col gap-5 sm:flex-row sm:items-end sm:justify-between">
              <div>
                <div className="mb-3 flex items-center gap-2 text-xs text-muted-foreground">
                  <ShieldCheck className="size-4" aria-hidden="true" />
                  <span>{accessLabel}</span>
                </div>
                <h1 className="text-[32px] font-semibold leading-none tracking-[-0.045em] sm:text-[38px]">{viewDetails[view].title}</h1>
                <p className="mt-4 max-w-2xl text-[15px] leading-6 text-muted-foreground">{viewDetails[view].description}</p>
              </div>
              {view !== "api-keys" || canManageActive ? (
                <a className="button-primary" href={`/dashboard?view=${view}&create=1#create-item`}><Plus aria-hidden="true" />New {view === "api-keys" ? "key" : view === "workspaces" ? "workspace" : "project"}</a>
              ) : null}
            </header>

            {view === "projects" ? (
              <ProjectsView
                me={me}
                workspaces={workspaces}
                activeWorkspace={activeWorkspace}
                canManageActive={canManageActive}
                showCreate={showCreate}
              />
            ) : null}
            {view === "workspaces" ? <WorkspacesView me={me} workspaces={workspaces} showCreate={showCreate} /> : null}
            {view === "api-keys" ? (
              <ApiKeysView me={me} keys={keys} canManageActive={canManageActive} activeWorkspace={activeWorkspace} activeProject={activeProject} showCreate={showCreate} />
            ) : null}
          </div>
        </section>
      </div>
    </main>
  );
}

function ProjectsView({
  me,
  workspaces,
  activeWorkspace,
  canManageActive,
  showCreate,
}: {
  me: Me;
  workspaces: Workspace[];
  activeWorkspace?: Workspace;
  canManageActive: boolean;
  showCreate: boolean;
}) {
  const projects = workspaces.flatMap((workspace) =>
    workspace.projects.map((project) => ({ ...project, workspace })),
  );

  return (
    <div className={showCreate ? "grid gap-6 xl:grid-cols-[minmax(0,1fr)_320px]" : "block"}>
      <div className="min-w-0">
        <FilterStrip items={["All workspaces", "All access levels"]} />
        <Panel className="metric-panel mb-5">
          <PanelHeader title="Resource utilization" description="Projects available to this account" />
          <MetricStrip label="Project inventory" value={projects.length} />
        </Panel>
        <Panel>
          <PanelHeader title="All projects" description={`${projects.length} across ${workspaces.length} workspace${workspaces.length === 1 ? "" : "s"}`} />
          {projects.length ? (
            <div className="overflow-x-auto">
              <table className="data-table">
                <thead><tr><th>Project</th><th>Workspace</th><th>Access</th><th>Status</th></tr></thead>
                <tbody>
                  {projects.map((project) => {
                    const active = project.id === me.project_id;
                    return (
                      <tr className={active ? "selected-row" : ""} key={project.id}>
                        <td><span className="row-title"><span className="row-icon"><Folder aria-hidden="true" /></span><span><strong>{project.name}</strong><small>{project.slug}</small></span></span></td>
                        <td data-label="Workspace">{project.workspace.name}</td>
                        <td className="capitalize" data-label="Access">{project.workspace.role}</td>
                        <td data-label="Status">
                          {active ? <Badge tone="accent">Selected</Badge> : (
                            <form action={selectContextAction}>
                              <input type="hidden" name="workspace_id" value={project.workspace.id} />
                              <input type="hidden" name="project_id" value={project.id} />
                              <button className="button-table" type="submit">Select</button>
                            </form>
                          )}
                        </td>
                      </tr>
                    );
                  })}
                </tbody>
              </table>
            </div>
          ) : <EmptyState icon={Folder} title="No projects yet" description="Create a workspace first; its default project will appear here." />}
        </Panel>
      </div>

      {showCreate ? (
        <Panel className="h-fit" id="create-item">
          <PanelHeader title="New project" description={activeWorkspace ? `Inside ${activeWorkspace.name}` : "Choose a workspace first"} action={<Link className="button-table" href="/dashboard?view=projects">Close</Link>} />
          <div className="p-5">
            {activeWorkspace && canManageActive ? (
              <SimpleForm action={createProjectAction} submitLabel="Create project">
                <input type="hidden" name="workspace_id" value={activeWorkspace.id} />
                <label className="field-label" htmlFor="project-name">Project name</label>
                <input className="field" id="project-name" name="name" placeholder="Production" minLength={2} required />
                <p className="field-help">The project becomes an authorization boundary for API keys and agents.</p>
              </SimpleForm>
            ) : (
              <p className="text-sm leading-6 text-muted-foreground">An owner or administrator can add projects to the selected workspace.</p>
            )}
          </div>
        </Panel>
      ) : null}
    </div>
  );
}

function WorkspacesView({ me, workspaces, showCreate }: { me: Me; workspaces: Workspace[]; showCreate: boolean }) {
  return (
    <div className={showCreate ? "grid gap-6 xl:grid-cols-[minmax(0,1fr)_320px]" : "block"}>
      <div className="min-w-0">
        <FilterStrip items={["All roles", "All workspace states"]} />
        <Panel className="metric-panel mb-5">
          <PanelHeader title="Tenant utilization" description="Workspace boundaries available to this account" />
          <MetricStrip label="Tenant inventory" value={workspaces.length} />
        </Panel>
        <Panel>
          <PanelHeader title="Your workspaces" description={`${workspaces.length} tenant boundar${workspaces.length === 1 ? "y" : "ies"}`} />
          {workspaces.length ? (
            <div className="overflow-x-auto">
              <table className="data-table">
                <thead><tr><th>Workspace</th><th>Role</th><th>Projects</th><th>Status</th></tr></thead>
                <tbody>
                  {workspaces.map((workspace) => (
                    <tr key={workspace.id}>
                      <td><span className="row-title"><span className="row-icon"><Building2 aria-hidden="true" /></span><span><strong>{workspace.name}</strong><small>{workspace.slug}</small></span></span></td>
                      <td className="capitalize" data-label="Role">{workspace.role}</td>
                      <td data-label="Projects">{workspace.projects.length}</td>
                      <td data-label="Status">
                        {workspace.id === me.workspace_id ? <Badge tone="accent">Current</Badge> : workspace.projects[0] ? (
                          <form action={selectContextAction}>
                            <input type="hidden" name="workspace_id" value={workspace.id} />
                            <input type="hidden" name="project_id" value={workspace.projects[0].id} />
                            <button className="button-table" type="submit">Switch</button>
                          </form>
                        ) : <Badge>Empty</Badge>}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          ) : <EmptyState icon={Building2} title="No workspace yet" description="Create your first tenant boundary to get a default project." />}
        </Panel>
      </div>

      {showCreate ? (
        <Panel className="h-fit" id="create-item">
          <PanelHeader title="New workspace" description="Includes a default project" action={<Link className="button-table" href="/dashboard?view=workspaces">Close</Link>} />
          <div className="p-5">
            <SimpleForm action={createWorkspaceAction} submitLabel="Create workspace">
              <label className="field-label" htmlFor="workspace-name">Workspace name</label>
              <input className="field" id="workspace-name" name="name" autoComplete="organization" placeholder="Acme Inc." minLength={2} required />
              <p className="field-help">Use the customer or team name people will recognize.</p>
            </SimpleForm>
          </div>
        </Panel>
      ) : null}
    </div>
  );
}

function ApiKeysView({
  me,
  keys,
  canManageActive,
  activeWorkspace,
  activeProject,
  showCreate,
}: {
  me: Me;
  keys: ApiKey[];
  canManageActive: boolean;
  activeWorkspace?: Workspace;
  activeProject?: Workspace["projects"][number];
  showCreate: boolean;
}) {
  return (
    <div className={showCreate ? "grid gap-6 xl:grid-cols-[minmax(0,1fr)_320px]" : "block"}>
      <div className="min-w-0">
        <FilterStrip items={[activeProject?.name ?? "Current project", "project:read"]} />
        <Panel className="metric-panel mb-5">
          <PanelHeader title="Credential activity" description="Keys authorized for the selected project" />
          <MetricStrip label="Credential inventory" value={keys.length} />
        </Panel>
        <Panel>
          <PanelHeader title="Project credentials" description={`${keys.length} active key${keys.length === 1 ? "" : "s"} for ${activeProject?.name ?? "this project"}`} />
          {keys.length ? (
            <div className="overflow-x-auto">
              <table className="data-table">
                <thead><tr><th>Name</th><th>Prefix</th><th>Scope</th><th>Created</th><th><span className="sr-only">Actions</span></th></tr></thead>
                <tbody>
                  {keys.map((key) => (
                    <tr key={key.id}>
                      <td><span className="row-title"><span className="row-icon"><KeyRound aria-hidden="true" /></span><strong>{key.name}</strong></span></td>
                      <td data-label="Prefix"><code>{key.prefix}…</code></td>
                      <td data-label="Scope"><Badge>{key.scopes.join(", ")}</Badge></td>
                      <td data-label="Created">{new Intl.DateTimeFormat("en", { dateStyle: "medium" }).format(new Date(key.created_at))}</td>
                      <td className="text-right" data-label="Actions">
                        {canManageActive ? (
                          <form action={revokeApiKeyAction}>
                            <input type="hidden" name="api_key_id" value={key.id} />
                            <ConfirmSubmitButton confirmMessage={`Revoke ${key.name}? Existing integrations using it will stop working.`} className="button-danger">Revoke</ConfirmSubmitButton>
                          </form>
                        ) : null}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
          ) : <EmptyState icon={KeyRound} title="No API keys yet" description="Create one only when a non-OAuth integration needs project access." />}
        </Panel>
      </div>

      {showCreate ? (
        <Panel className="h-fit" id="create-item">
          <PanelHeader title="New API key" description="Shown once, then hashed" action={<Link className="button-table" href="/dashboard?view=api-keys">Close</Link>} />
          <div className="p-5">
            {canManageActive ? (
              <SimpleForm action={createApiKeyAction} submitLabel="Create API key">
                <input type="hidden" name="project_id" value={me.project_id} />
                <div className="scope-summary"><span>Target</span><strong>{activeWorkspace?.name ?? "Workspace"} / {activeProject?.name ?? "Project"}</strong><span>Scope</span><strong>project:read</strong></div>
                <label className="field-label" htmlFor="api-key-name">Key name</label>
                <input className="field" id="api-key-name" name="name" placeholder="Local development" minLength={2} required />
                <p className="field-help">Name it after the integration, machine, or environment that will use it.</p>
              </SimpleForm>
            ) : <p className="text-sm leading-6 text-muted-foreground">An owner or administrator can create project API keys.</p>}
          </div>
        </Panel>
      ) : null}
    </div>
  );
}

function SidebarLink({ href, active, icon: Icon, children }: { href: string; active: boolean; icon: typeof Folder; children: React.ReactNode }) {
  return (
    <Link className={`sidebar-link ${active ? "sidebar-link-active" : ""}`} href={href} aria-current={active ? "page" : undefined}>
      <Icon aria-hidden="true" />{children}{active ? <><span className="active-dot" /><span className="sr-only"> (current)</span></> : null}
    </Link>
  );
}

function SidebarAnchor({ href, icon: Icon, children }: { href: string; icon: typeof Folder; children: React.ReactNode }) {
  return <a className="sidebar-link resource-link" href={href}><Icon aria-hidden="true" />{children}<ExternalLink className="ml-auto size-3.5 opacity-40" aria-hidden="true" /></a>;
}

// Component vocabulary adapted from UI by Halaska (MIT): https://github.com/Halaska-Studio/ui
function Panel({ children, className = "", id }: { children: React.ReactNode; className?: string; id?: string }) {
  return <section className={`overflow-hidden rounded-2xl border border-border bg-card shadow-[0_1px_1px_rgb(0_0_0/0.02)] ${className}`} id={id}>{children}</section>;
}

function PanelHeader({ title, description, action }: { title: string; description: string; action?: React.ReactNode }) {
  return <header className="flex min-h-[92px] items-center justify-between gap-4 border-b border-border px-6"><div><h2 className="text-[15px] font-semibold tracking-[-0.01em]">{title}</h2><p className="mt-1.5 text-xs text-muted-foreground">{description}</p></div>{action}</header>;
}

function FilterStrip({ items }: { items: string[] }) {
  return <div className="filter-strip" aria-label="Current inventory scope">{items.map((item) => <span key={item}>{item}</span>)}</div>;
}

function MetricStrip({ label, value }: { label: string; value: number }) {
  return (
    <div className="metric-strip">
      <div><span>{label}</span><strong>{value}</strong></div>
      <span className="metric-bars" aria-hidden="true">
        {Array.from({ length: 36 }, (_, index) => <i className={index >= 36 - Math.min(value, 8) ? "hot" : ""} key={index} />)}
      </span>
    </div>
  );
}

function Badge({ children, tone = "neutral" }: { children: React.ReactNode; tone?: "neutral" | "accent" }) {
  return <span className={`badge ${tone === "accent" ? "badge-accent" : ""}`}>{children}</span>;
}

function EmptyState({ icon: Icon, title, description }: { icon: typeof Folder; title: string; description: string }) {
  return <div className="grid min-h-[300px] place-items-center p-8 text-center"><div className="max-w-xs"><span className="mx-auto grid size-11 place-items-center rounded-[14px] border border-border bg-subtle text-muted-foreground"><Icon className="size-5" aria-hidden="true" /></span><h3 className="mt-4 text-sm font-semibold">{title}</h3><p className="mt-2 text-sm leading-6 text-muted-foreground">{description}</p></div></div>;
}

function BrandMark() {
  return <span className="brand-mark" aria-hidden="true">✦</span>;
}

function initials(value: string) {
  return value.split(/\s+/).slice(0, 2).map((part) => part[0]).join("").toUpperCase();
}
