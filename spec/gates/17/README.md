# Gate 17 — Python 3.12 Drop-In Release Qualification

Gate 17 is the only gate allowed to authorize a drop-in compatibility
guarantee. It adds no alternate semantics. It audits the completed compiler,
runtime, standard library, extension ABI, packaging system, and supported
platforms as one product and rejects release while any required public surface
is missing or has an unexplained CPython 3.12 divergence.

## Guarantee contract

A qualified Rimera release guarantees that an application runs without
Rimera-specific source changes when all of the following are true:

- the application is valid for Python 3.12 and does not depend on undefined
  behavior or undocumented CPython internals outside the declared ABI tier;
- its target operating system and architecture appear in the signed support
  matrix;
- every standard-library/platform facility it uses is marked supported;
- each third-party native extension uses an ABI tier the release implements;
- required filesystem, process, network, dynamic-code, and native-code
  capabilities are granted; and
- its declared resource requirements fit the configured limits.

Within that scope, a crash, incorrect result, unexplained exception difference,
import/install failure, or requirement for a Rimera-only source branch is a
Rimera compatibility defect—not an application limitation.

No finite test corpus can prove every possible program mathematically. This
gate converts the scope above into an enforceable release warranty backed by
complete public-surface inventories, differential suites, fuzzing, package
corpora, and per-target continuous qualification.

## Progress ledger

- [ ] Slice 1 — normative guarantee, exclusions, target/ABI matrix, and release policy
- [ ] Slice 2 — complete Python 3.12 grammar/data-model and CPython regression qualification
- [ ] Slice 3 — complete stdlib public-surface and platform-behavior qualification
- [ ] Slice 4 — launcher, environment, packaging, import, and tooling substitution
- [ ] Slice 5 — extension ABI symbol/type/lifecycle and binary-wheel qualification
- [ ] Slice 6 — unmodified application corpus and cross-version dependency qualification
- [ ] Slice 7 — differential fuzzing, stress, security, recovery, and long-run stability
- [ ] Slice 8 — reproducible release audit, signed compatibility manifest, and guarantee

## Slice acceptance

1. Freeze the exact Python patch oracle, operating systems, architectures,
   capability defaults, stdlib tier, extension ABI, wheel tags, resource model,
   exclusions, support lifetime, and severity policy. Ambiguous exclusions are
   forbidden.
2. Run the applicable upstream CPython grammar, data-model, builtin, exception,
   GC, reflection, import, async, and runtime regression suites with zero
   unexplained failures; every skip maps to a published out-of-scope item.
3. Inventory every public stdlib module, attribute, function, class, constant,
   error, and platform branch for the target. Import-only success is
   insufficient; applicable upstream behavior suites must pass.
4. Prove ordinary Python invocation expectations, `sys.path` and environment
   behavior, scripts/entry points, subprocess reinvocation, build/test tools,
   wheel/sdist installation, resources, and isolated/reproducible resolution.
5. For the declared extension tier, verify exported symbols, structures, slots,
   ownership, errors, buffers, GC, callbacks, threads, module initialization,
   shutdown, and matching binary wheels. An incomplete CPython ABI cannot be
   advertised as CPython binary compatibility.
6. Continuously install and run pinned, unmodified applications across CLI,
   web, data, scientific, serialization, build, testing, and async workloads;
   framework-specific compiler branches are forbidden.
7. Differentially fuzz syntax and runtime states, stress long-lived and
   concurrent workloads, exercise resource exhaustion and hostile inputs, and
   prove failures do not corrupt caches, outputs, heaps, or future runs.
8. Rebuild from locked inputs on clean hosts, run every gate boundary on every
   supported target, publish a signed machine-readable compatibility manifest,
   and issue the guarantee only when no required surface is partial.

Gate 17 closure permits the wording “Python 3.12 drop-in replacement” only with
the qualified release's published target, capability, stdlib, and extension-
ABI scope attached. “Runs every Python program everywhere” is never an honest
or testable product claim.

The first drop-in manifest must include the Gate 16 Flask, FastAPI, discord.py,
and Pycord profiles with exact versions, extras, dependency locks, targets, and
test results. A profile omitted from the signed manifest is not guaranteed,
even if a smaller example happened to run during development.
