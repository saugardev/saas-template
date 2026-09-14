const features = [
  ["Identity", "Password auth, generic social OIDC, and opaque browser sessions."],
  ["Tenancy", "Workspace and project isolation with role-aware access and scoped API keys."],
  ["Agent access", "OAuth 2.1, OIDC, protected-resource discovery, and Streamable HTTP MCP."],
] as const;

export default function Home() {
  const appUrl = process.env.APP_URL ?? "http://localhost:3000";
  const docsUrl = process.env.DOCS_URL ?? "http://localhost:3003";

  return (
    <main className="min-h-screen px-5 py-6 sm:px-8">
      <nav className="mx-auto flex max-w-6xl items-center justify-between" aria-label="Primary">
        <a
          href="#top"
          className="inline-flex min-h-10 items-center rounded-sm text-sm font-semibold tracking-tight focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--ring)]"
        >
          Agent SaaS Starter
        </a>
        <div className="flex items-center gap-2">
          <a
            href={docsUrl}
            className="inline-flex min-h-10 items-center rounded-lg px-3 py-2 text-sm font-medium text-[var(--muted)] hover:text-[var(--foreground)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--ring)]"
          >
            Docs
          </a>
          <a
            href={appUrl}
            className="inline-flex min-h-10 items-center rounded-lg bg-[var(--accent)] px-4 py-2 text-sm font-medium text-[var(--accent-foreground)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--ring)] focus-visible:ring-offset-2"
          >
            Open app
          </a>
        </div>
      </nav>

      <section id="top" className="mx-auto grid min-w-0 max-w-6xl gap-12 pb-20 pt-24 lg:grid-cols-[1.25fr_0.75fr] lg:items-end lg:pt-36">
        <div className="min-w-0">
          <p className="mb-5 font-mono text-xs uppercase tracking-[0.18em] text-[var(--muted)]">
            Multi-tenant SaaS · OAuth · MCP
          </p>
          <h1 className="max-w-4xl text-4xl font-semibold leading-[1.04] tracking-[-0.04em] sm:text-7xl sm:leading-[1.02] sm:tracking-[-0.045em]">
            The unglamorous foundation, already done well.
          </h1>
        </div>
        <div className="max-w-md lg:pb-2">
          <p className="text-lg leading-8 text-[var(--muted)]">
            Start with secure identity, tenant boundaries, and agent authorization. Add only the product that makes yours different.
          </p>
          <div className="mt-7 flex flex-wrap gap-3">
            <a href={appUrl} className="inline-flex min-h-11 items-center rounded-lg bg-[var(--accent)] px-5 py-3 text-sm font-medium text-[var(--accent-foreground)] focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--ring)] focus-visible:ring-offset-2">
              Start locally
            </a>
            <a href={docsUrl} className="inline-flex min-h-11 items-center rounded-lg border border-[var(--border)] bg-[var(--surface)] px-5 py-3 text-sm font-medium focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-[var(--ring)]">
              Read the guide
            </a>
          </div>
        </div>
      </section>

      <section className="mx-auto grid max-w-6xl border-y border-[var(--border)] md:grid-cols-3" aria-labelledby="included-heading">
        <h2 id="included-heading" className="sr-only">Included foundations</h2>
        {features.map(([title, description], index) => (
          <article key={title} className={`py-8 md:px-7 ${index > 0 ? "border-t border-[var(--border)] md:border-l md:border-t-0" : ""}`}>
            <p className="font-mono text-xs text-[var(--muted)]">0{index + 1}</p>
            <h3 className="mt-8 text-lg font-semibold">{title}</h3>
            <p className="mt-2 max-w-sm text-sm leading-6 text-[var(--muted)]">{description}</p>
          </article>
        ))}
      </section>

      <footer className="mx-auto flex max-w-6xl flex-col gap-3 py-10 text-sm text-[var(--muted)] sm:flex-row sm:items-center sm:justify-between">
        <p>Brand-neutral by design.</p>
        <p>Next.js · Rust · PostgreSQL</p>
      </footer>
    </main>
  );
}
