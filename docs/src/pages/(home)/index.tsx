import { Link } from 'waku';

export default function Home() {
  return (
    <main className="flex-1">
      <section className="mx-auto flex max-w-5xl flex-col items-center px-6 pb-24 pt-24 text-center sm:pt-32">
        <p className="mb-6 rounded-full border border-fd-border bg-fd-card px-3 py-1 text-xs font-medium text-fd-muted-foreground">
          Native Python, built for the machine
        </p>
        <h1 className="max-w-3xl text-5xl font-semibold tracking-tight text-fd-foreground sm:text-7xl">
          Small miracles, compiled.
        </h1>
        <p className="mt-6 max-w-2xl text-lg leading-8 text-fd-muted-foreground">
          Rimera turns Python source into standalone native executables through a
          Rust-owned compiler, Cranelift, and a compact runtime.
        </p>
        <div className="mt-9 flex flex-wrap justify-center gap-3">
          <Link
            to="/docs"
            className="rounded-lg bg-fd-primary px-4 py-2.5 text-sm font-medium text-fd-primary-foreground transition-opacity hover:opacity-90"
          >
            Read the docs
          </Link>
          <a
            href="https://github.com/Nadhila-dot/Rimera-lite"
            className="rounded-lg border border-fd-border px-4 py-2.5 text-sm font-medium transition-colors hover:bg-fd-accent"
          >
            View source
          </a>
        </div>
      </section>
      <section className="mx-auto grid max-w-5xl gap-4 px-6 pb-24 sm:grid-cols-3">
        {[
          ['Native pipeline', 'Python → HIR → MIR → Cranelift → executable'],
          ['Honest compatibility', 'Every supported feature is proven end to end.'],
          ['Fast feedback', 'Clear diagnostics, cached objects, and a focused CLI.'],
        ].map(([title, description]) => (
          <div key={title} className="rounded-xl border border-fd-border bg-fd-card p-5">
            <h2 className="font-medium text-fd-foreground">{title}</h2>
            <p className="mt-2 text-sm leading-6 text-fd-muted-foreground">{description}</p>
          </div>
        ))}
      </section>
    </main>
  );
}

export async function getConfig() {
  return {
    render: 'static',
  };
}
