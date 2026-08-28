import Link from "next/link";

export function AuthShell({
  title,
  description,
  children,
}: {
  title: string;
  description: string;
  children: React.ReactNode;
}) {
  return (
    <main className="grid min-h-screen lg:grid-cols-[minmax(0,1fr)_minmax(28rem,0.8fr)]">
      <section className="hidden border-r border-border bg-muted/50 p-10 lg:flex lg:flex-col lg:justify-between">
        <Link
          className="inline-flex min-h-10 items-center rounded-sm text-sm font-semibold tracking-tight focus-visible:ring-2 focus-visible:ring-ring"
          href="/"
        >
          Agent SaaS Starter
        </Link>
        <div className="max-w-lg space-y-5">
          <p className="font-mono text-xs uppercase tracking-[0.18em] text-muted-foreground">
            Identity → consent → tools
          </p>
          <h2 className="text-4xl font-semibold tracking-tight">
            A quiet foundation for software agents.
          </h2>
          <p className="max-w-md text-base leading-7 text-muted-foreground">
            First-party auth, tenant boundaries, and standards-based MCP access
            without product-specific baggage.
          </p>
        </div>
        <p className="text-xs text-muted-foreground">
          OAuth 2.1 · OpenID Connect · MCP
        </p>
      </section>
      <section className="flex items-center justify-center px-4 py-12 sm:px-8">
        <div className="w-full max-w-sm">
          <div className="mb-8 space-y-2">
            <h1 className="text-3xl font-semibold tracking-tight">{title}</h1>
            <p className="leading-6 text-muted-foreground">{description}</p>
          </div>
          {children}
        </div>
      </section>
    </main>
  );
}
