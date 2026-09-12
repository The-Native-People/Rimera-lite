import { HomeShader } from '@/components/home-shader';
import { TerminalSequence } from '@/components/terminal-sequence';
import { ArrowRight, ArrowUpRight, Braces, CircleStop, CornerDownRight, FileCode2 } from 'lucide-react';
import type { ReactNode } from 'react';
import { Link } from 'waku';

const terminalOutput = {
  0: [
    'Rimera Lite [♣]',
    '  |-+ source  tests/fixtures/basic/hello.py',
    '  |-+ target  aarch64-apple-darwin · debug',
    '  |-+ output  tests/fixtures/basic/.rimera/bin/hello',
    '  |-+ linker  clang',
    '',
    '  ○  1/5  Read source',
    '  ✓  1/5  Read source        [=========] 100%  15 B / 15 B  ·  1 statements · 1 lines',
    '  ○  2/5  Analyze semantics',
    '  ✓  2/5  Analyze semantics  [=========] 100%  1 HIR statement / 1 HIR statement  ·  analyzed',
    '  ○  3/5  Lower native IR',
    '  ✓  3/5  Lower native IR    [=========] 100%  1 native function / 1 native function  ·  1 blocks · 0 values',
    '  ○  4/5  Emit object',
    '  ✓  4/5  Emit object        [=========] 100%  1 object / 1 object  ·  1.53 KiB (1568 B) · 1 native functions',
    '  ○  5/5  Link with clang',
    '  ✓  5/5  Link with clang    [=========] 100%  1 executable / 1 executable  ·  clang linked · 1.62 MiB (1703656 B)',
    '',
    'Done in 0.32s +|+ Size: 1703656 bytes.',
    '',
    'Running [▹]   tests/fixtures/basic/.rimera/bin/hello',
    'hello',
  ],
};

const actionBase =
  'inline-flex min-h-11 items-center justify-center gap-2 rounded-full px-5 text-[0.78rem] font-bold no-underline transition duration-200 ease-out hover:-translate-y-0.5 active:translate-y-0 focus-visible:outline-2 focus-visible:outline-offset-4 focus-visible:outline-rimera-accent';

export default function Home() {
  return (
    <main className="mx-auto w-[min(calc(100%-1rem),118rem)] pb-28 pt-2 text-rimera-ink transition-colors duration-300 sm:w-[min(calc(100%-2.5rem),118rem)] sm:pt-6">
      <section
        className="relative isolate overflow-hidden rounded-[1.15rem] border border-rimera-line bg-rimera-hero outline outline-1 outline-offset-[5px] outline-rimera-ring transition-colors duration-300 sm:rounded-[1.5rem]"
        aria-labelledby="rimera-title"
      >
        <HomeShader />
        <div
          className="pointer-events-none absolute inset-0 -z-10 [background:var(--rimera-hero-shade)]"
          aria-hidden="true"
        />

        <div className="max-w-[53rem] px-5 pb-8 pt-12 sm:px-[clamp(2rem,4.5vw,5rem)] sm:pb-9 sm:pt-[clamp(3.5rem,5vw,5rem)]">
          <h1
            id="rimera-title"
            className="max-w-[12ch] [font-family:var(--font-display)] text-[clamp(2.8rem,3.6vw,4.65rem)] font-[650] leading-[0.93] tracking-[-0.06em] text-rimera-ink"
          >
            Compile Python.<br />Run <span className="text-rimera-accent">native.</span>
          </h1>
          <p className="mt-5 max-w-[61ch] text-[clamp(0.76rem,0.85vw,0.9rem)] leading-6 text-rimera-muted">
            Rimera Lite turns supported Python into machine code and links it
            with a compact Rust runtime. One source file becomes one executable.
          </p>
          <div className="mt-6 flex flex-wrap gap-3">
            <Link
              to="/docs/getting-started"
              className={`${actionBase} bg-rimera-accent-soft text-rimera-on-accent hover:brightness-105`}
            >
              Get started <CornerDownRight size={15} strokeWidth={2.2} />
            </Link>
            <a
              href="https://github.com/Nadhila-dot/Rimera-lite"
              className={`${actionBase} border border-rimera-line bg-rimera-surface/80 text-rimera-ink outline outline-1 outline-offset-[5px] outline-rimera-ring hover:bg-rimera-surface-raised`}
            >
              View source <ArrowUpRight size={15} strokeWidth={2.2} />
            </a>
          </div>
        </div>

        <div className="relative mx-auto -mb-px w-[calc(100%-2rem)] max-w-[80rem] overflow-hidden rounded-t-[1rem] border border-b-0 border-white/12 bg-[#0a0e13]/96 shadow-[0_-1rem_5rem_rgba(0,0,0,0.28)] backdrop-blur-xl sm:w-[calc(100%-3rem)]">
          <div className="flex min-h-12 items-center justify-between gap-4 border-b border-white/10 px-4 text-[0.64rem] tracking-[0.03em] text-[#7f9199] sm:px-5">
            <span className="flex items-center gap-1.5" aria-hidden="true">
              <span className="size-2 rounded-full bg-[#f07167]" />
              <span className="size-2 rounded-full bg-[#d7b85a]" />
              <span className="size-2 rounded-full bg-[#6fc29d]" />
            </span>
            <span className="absolute left-1/2 -translate-x-1/2 text-[#9aabb1]">kitty ~</span>
            <span>aarch64-apple-darwin</span>
          </div>
          <TerminalSequence
            commands={['rimera-lite tests/fixtures/basic/hello.py --run']}
            outputs={terminalOutput}
            username="nadhi@Mac Rimera-lite"
            typingSpeed={12}
            initialDelay={160}
          />
        </div>
      </section>

      <section className="px-1 py-28 sm:px-[clamp(0rem,3vw,3rem)] sm:py-44" aria-label="What Rimera is">
        <p className="max-w-[50ch] [font-family:var(--font-display)] text-[clamp(2.1rem,4.1vw,4.8rem)] font-[480] leading-[1.18] tracking-[-0.055em] text-rimera-ink">
          Rimera Lite is a <strong className="font-[650] text-rimera-accent">native Python compiler</strong> for supported programs. It emits real object files and makes its <strong className="font-[650] text-rimera-strong">compatibility boundary explicit.</strong>
        </p>
      </section>

      <section className="grid gap-5 lg:grid-cols-2" aria-label="Rimera compiler details">
        <FeatureArticle>
          <Eyebrow>01 / DIRECT COMPILER</Eyebrow>
          <Braces className="mt-10 text-rimera-accent" size={30} strokeWidth={1.4} />
          <h2 className="mt-6 max-w-[17ch] [font-family:var(--font-display)] text-[clamp(2.2rem,3.4vw,3.8rem)] font-semibold leading-none tracking-[-0.055em]">
            One route from source to executable.
          </h2>
          <p className="mt-6 max-w-[54ch] text-sm leading-7 text-rimera-muted">
            No generated C, bytecode VM, or silent fallback. Every supported
            operation crosses the same verified native pipeline.
          </p>
          <FeatureLink to="/docs/how-rimera-works">See how compilation works</FeatureLink>
        </FeatureArticle>

        <article className="flex min-h-[32rem] flex-col justify-between rounded-[1.35rem] border border-[#b7d6e2]/30 bg-[#d9edf2] p-5 text-[#071017] sm:p-8">
          <div className="overflow-hidden rounded-xl border border-[#273943] bg-[#091016] text-[#dbe6e8] shadow-[0.45rem_0.45rem_0_rgba(22,124,163,0.16)]">
            <div className="flex items-center justify-between gap-3 border-b border-white/10 px-4 py-3 text-[0.65rem] text-[#8fa0a6]">
              <span className="flex items-center gap-2.5">
                <FileCode2 size={16} strokeWidth={1.5} className="text-[#8adcf7]" />
                app.py
              </span>
              <span>Python</span>
            </div>
            <code className="block px-5 py-7 text-[clamp(0.7rem,1.1vw,0.84rem)] leading-7 text-[#aebbc0] sm:px-7 sm:py-8">
              <span className="text-[#8adcf7]">def</span> greet(name):<br />
              &nbsp;&nbsp;print(<span className="text-[#9bdabf]">f&quot;Hello, {'{'}name{'}'}!&quot;</span>)<br />
              <br />
              greet(<span className="text-[#9bdabf]">&quot;Rimera&quot;</span>)
            </code>
            <div className="flex items-center gap-3 border-t border-white/10 px-5 py-3 text-[0.62rem] text-[#809198] sm:px-7">
              <span>Python source</span><ArrowRight size={13} /><strong className="font-semibold text-[#9bdabf]">native executable</strong>
            </div>
          </div>
          <p className="mt-8 max-w-[34ch] [font-family:var(--font-display)] text-xl font-semibold leading-tight tracking-[-0.03em] sm:text-2xl">
            Keep writing Python. Rimera carries it to native code.
          </p>
        </article>

        <article className="flex min-h-[32rem] flex-col justify-between rounded-[1.35rem] border border-[#b7d6e2]/30 bg-[#d9edf2] p-5 text-[#071017] sm:p-8">
          <div className="overflow-hidden rounded-xl border border-[#273943] bg-[#091016] text-[#dbe6e8] shadow-[0.75rem_0.75rem_0_rgba(22,124,163,0.2)]">
            <div className="flex justify-between gap-4 border-b border-white/10 px-4 py-3 text-[0.64rem] text-[#7e9199]">
              <span>pyproject.toml</span><span>[tool.rimera]</span>
            </div>
            <code className="block px-5 py-8 text-[clamp(0.7rem,1.2vw,0.86rem)] leading-8 text-[#aebbc0] sm:px-8 sm:py-10">
              output = <em className="not-italic text-[#8adcf7]">&quot;dist/app&quot;</em><br />
              profile = <em className="not-italic text-[#8adcf7]">&quot;release&quot;</em><br />
              heap_limit_bytes = <em className="not-italic text-[#8adcf7]">33554432</em>
            </code>
          </div>
          <p className="mt-10 max-w-[32ch] [font-family:var(--font-display)] text-2xl font-semibold leading-tight tracking-[-0.035em] sm:text-3xl">
            Project settings stay beside the Python they build.
          </p>
        </article>

        <FeatureArticle>
          <Eyebrow>03 / CLEAR FAILURE</Eyebrow>
          <CircleStop className="mt-10 text-rimera-accent" size={30} strokeWidth={1.4} />
          <h2 className="mt-6 max-w-[17ch] [font-family:var(--font-display)] text-[clamp(2.2rem,3.4vw,3.8rem)] font-semibold leading-none tracking-[-0.055em]">
            Unsupported means stopped.
          </h2>
          <p className="mt-6 max-w-[54ch] text-sm leading-7 text-rimera-muted">
            Rimera reports the source span and removes partial output instead
            of quietly changing the program&apos;s meaning.
          </p>
          <FeatureLink to="/docs/supported-python">Check supported Python</FeatureLink>
        </FeatureArticle>
      </section>

      <section className="px-1 pb-8 pt-32 sm:px-[clamp(0rem,3vw,3rem)] sm:pt-52" aria-labelledby="rimera-start-title">
        <Eyebrow>READY / START WITH THE BINARY</Eyebrow>
        <h2 id="rimera-start-title" className="mt-7 max-w-[13ch] [font-family:var(--font-display)] text-[clamp(3.4rem,7vw,8rem)] font-semibold leading-[0.9] tracking-[-0.07em] text-rimera-ink">
          One command.<br /><span className="text-rimera-accent">One executable.</span>
        </h2>
        <div className="mt-12 flex flex-col items-start justify-between gap-5 border-t border-rimera-line pt-6 sm:flex-row sm:items-center">
          <code className="text-[clamp(0.7rem,1.4vw,0.92rem)] text-rimera-accent">rimera-lite app.py --output dist/app</code>
          <Link to="/docs/getting-started" className={`${actionBase} border border-rimera-line bg-rimera-surface text-rimera-ink hover:bg-rimera-surface-raised`}>
            Open the guide <ArrowUpRight size={15} />
          </Link>
        </div>
      </section>
    </main>
  );
}

function Eyebrow({ children }: { children: ReactNode }) {
  return <p className="text-[0.66rem] font-bold tracking-[0.1em] text-rimera-accent">{children}</p>;
}

function FeatureArticle({ children }: { children: ReactNode }) {
  return <article className="relative min-h-[32rem] overflow-hidden rounded-[1.35rem] border border-rimera-line bg-rimera-surface p-7 transition-colors duration-300 sm:p-12">{children}</article>;
}

function FeatureLink({ to, children }: { to: string; children: ReactNode }) {
  return (
    <Link to={to} className="group absolute bottom-8 left-7 inline-flex items-center gap-2 text-[0.72rem] font-bold text-rimera-ink no-underline focus-visible:outline-2 focus-visible:outline-offset-4 focus-visible:outline-rimera-accent sm:bottom-12 sm:left-12">
      {children}<ArrowUpRight className="transition-transform group-hover:translate-x-0.5 group-hover:-translate-y-0.5" size={15} />
    </Link>
  );
}

export async function getConfig() {
  return { render: 'static' };
}
