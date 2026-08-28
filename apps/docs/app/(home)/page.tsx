import Link from 'next/link';

export default function HomePage() {
  const appUrl = process.env.APP_URL ?? 'http://localhost:3000';

  return (
    <main className="mx-auto flex w-full max-w-5xl flex-1 flex-col justify-center px-6 py-20">
      <p className="mb-5 font-mono text-xs uppercase tracking-[0.18em] text-fd-muted-foreground">
        SaaS · OAuth · OpenID Connect · MCP
      </p>
      <h1 className="max-w-4xl text-5xl font-semibold leading-[1.02] tracking-[-0.04em] sm:text-7xl">
        Build the product, not its identity plumbing.
      </h1>
      <p className="mt-7 max-w-2xl text-lg leading-8 text-fd-muted-foreground">
        A practical guide to the starter&apos;s tenant model, browser auth, agent authorization server, and protected Streamable HTTP MCP endpoint.
      </p>
      <div className="mt-9 flex flex-wrap gap-3">
        <Link href="/docs" className="rounded-lg bg-fd-primary px-5 py-3 text-sm font-medium text-fd-primary-foreground focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-fd-ring">
          Read the quickstart
        </Link>
        <a href={appUrl} className="rounded-lg border px-5 py-3 text-sm font-medium focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-fd-ring">
          Open local app
        </a>
      </div>
    </main>
  );
}
