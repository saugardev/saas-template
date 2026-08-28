export default function DashboardLoading() {
  return (
    <main className="mx-auto w-full max-w-7xl px-4 py-10 sm:px-6 lg:px-8" aria-label="Loading dashboard">
      <div className="mb-10 h-24 max-w-2xl animate-pulse rounded-xl bg-muted motion-reduce:animate-none" />
      <div className="grid gap-6 lg:grid-cols-3">
        <div className="h-72 animate-pulse rounded-xl bg-muted motion-reduce:animate-none lg:col-span-2" />
        <div className="h-72 animate-pulse rounded-xl bg-muted motion-reduce:animate-none" />
        <div className="h-64 animate-pulse rounded-xl bg-muted motion-reduce:animate-none lg:col-span-3" />
      </div>
    </main>
  );
}

