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
    <main className="min-h-screen bg-background p-2 sm:p-4">
      <div className="mx-auto grid min-h-[calc(100svh-1rem)] max-w-[1260px] overflow-hidden rounded-[22px] border border-border bg-card shadow-[0_1px_2px_rgb(0_0_0/0.04),0_18px_50px_rgb(0_0_0/0.06)] sm:min-h-[calc(100svh-2rem)] sm:rounded-[28px] lg:grid-cols-[1.05fr_0.95fr]">
        <section className="hidden border-r border-border bg-subtle p-10 lg:flex lg:flex-col lg:justify-between xl:p-14">
          <Link className="wordmark" href="/">
            <span className="brand-mark" aria-hidden="true">✦</span>
            <span>Agent SaaS</span>
          </Link>
          <div className="max-w-lg">
            <p className="font-mono text-[11px] uppercase tracking-[0.14em] text-muted-foreground">
              Identity → consent → tools
            </p>
            <h2 className="mt-5 text-[48px] font-semibold leading-[1.02] tracking-[-0.055em]">
              A quiet foundation for software agents.
            </h2>
            <p className="mt-5 max-w-md text-[15px] leading-7 text-muted-foreground">
              First-party auth, tenant boundaries, and standards-based MCP access without product-specific baggage.
            </p>
            <div className="mt-10 overflow-hidden rounded-2xl bg-[#202021] text-white shadow-[0_12px_30px_rgb(0_0_0/0.14)]">
              <div className="auth-art" />
              <div className="p-5">
                <strong className="text-sm font-medium">One narrow grant.</strong>
                <p className="mt-2 text-xs leading-5 text-[#adadaf]">User, workspace, project, role, and scope stay visible all the way to the tool.</p>
              </div>
            </div>
          </div>
          <p className="font-mono text-[10px] uppercase tracking-[0.1em] text-muted-foreground">OAuth 2.1 · OpenID Connect · MCP</p>
        </section>
        <section className="flex items-center justify-center px-5 py-12 sm:px-8">
          <div className="w-full max-w-[380px]">
            <Link className="wordmark mb-12 lg:hidden" href="/">
              <span className="brand-mark" aria-hidden="true">✦</span>
              <span>Agent SaaS</span>
            </Link>
            <div className="mb-8 space-y-2">
              <h1 className="text-[32px] font-semibold tracking-[-0.045em]">{title}</h1>
              <p className="text-sm leading-6 text-muted-foreground">{description}</p>
            </div>
            {children}
          </div>
        </section>
      </div>
    </main>
  );
}
