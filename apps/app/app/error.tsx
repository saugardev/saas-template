"use client";

export default function GlobalError({ reset }: { error: Error & { digest?: string }; reset: () => void }) {
  return (
    <main className="flex min-h-screen items-center justify-center px-4">
      <div className="max-w-md rounded-xl border border-border bg-card p-6">
        <h1 className="text-xl font-semibold">Couldn’t load this page</h1>
        <p className="mt-2 text-sm leading-6 text-muted-foreground">
          The API may still be starting, or the network had a brief interruption.
        </p>
        <button
          className="mt-5 min-h-11 rounded-lg bg-primary px-4 text-sm font-semibold text-primary-foreground focus-visible:ring-2 focus-visible:ring-ring"
          type="button"
          onClick={reset}
        >
          Try again
        </button>
      </div>
    </main>
  );
}

