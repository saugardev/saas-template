export default function DashboardLoading() {
  return (
    <main className="min-h-screen bg-background p-2 sm:p-4" aria-label="Loading dashboard">
      <div className="mx-auto grid min-h-[calc(100svh-1rem)] max-w-[1600px] overflow-hidden rounded-[22px] border border-border bg-card sm:min-h-[calc(100svh-2rem)] sm:rounded-[28px] md:grid-cols-[276px_minmax(0,1fr)]">
        <aside className="hidden border-r border-border p-5 md:block">
          <div className="h-10 w-36 animate-pulse rounded-xl bg-muted motion-reduce:animate-none" />
          <div className="mt-8 h-14 animate-pulse rounded-xl bg-muted motion-reduce:animate-none" />
          <div className="mt-8 space-y-2">
            {Array.from({ length: 5 }, (_, index) => <div className="h-11 animate-pulse rounded-xl bg-muted motion-reduce:animate-none" key={index} />)}
          </div>
        </aside>
        <section className="min-w-0 bg-[#fdfdfc]">
          <div className="h-[72px] border-b border-border" />
          <div className="p-5 sm:p-9">
            <div className="h-20 max-w-xl animate-pulse rounded-2xl bg-muted motion-reduce:animate-none" />
            <div className="mt-8 grid gap-6 xl:grid-cols-[minmax(0,1fr)_320px]">
              <div className="h-[440px] animate-pulse rounded-2xl bg-muted motion-reduce:animate-none" />
              <div className="h-72 animate-pulse rounded-2xl bg-muted motion-reduce:animate-none" />
            </div>
          </div>
        </section>
      </div>
    </main>
  );
}
