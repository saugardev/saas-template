"use client";

export default function GlobalError({ reset }: { error: Error & { digest?: string }; reset: () => void }) {
  return (
    <main className="flex min-h-screen items-center justify-center bg-background px-4">
      <div className="max-w-md rounded-2xl border border-border bg-card p-7 shadow-[0_14px_40px_rgb(0_0_0/0.07)]">
        <h1 className="text-xl font-semibold">Couldn’t load this page</h1>
        <p className="mt-2 text-sm leading-6 text-muted-foreground">
          The API may still be starting, or the network had a brief interruption.
        </p>
        <button
          className="button-primary mt-5 min-h-11"
          type="button"
          onClick={reset}
        >
          Try again
        </button>
      </div>
    </main>
  );
}
