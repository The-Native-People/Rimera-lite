# Gate 10 Slice 10 — Compio Adapter, Static Linking, CLI Selection, and Diagnostics

## Goal

Ship Compio as the first executor backend without leaking backend details into
Python semantics or adding async cost to synchronous executables.

## Integrated work

- Implement the Slice 4/5 facade with Compio's local runtime, task spawning,
  wakers, timers, and completion facilities; retain Rimera's explicitly defined
  cancellation state around those primitives. Do not wrap Compio in a Rimera
  ready queue or a second worker pool.
- Select exactly one backend at build/link time. Use monomorphized/static calls
  in the poll path and link only the selected backend archive and platform libs.
- Add `--async <auto|compio|monoio|tokio>` and the equivalent project
  setting. CLI overrides project configuration; `auto` selects Compio when the
  program reaches async execution. Monoio and Tokio remain recognized but
  unavailable until their adapters and target evidence exist.
- Report an unavailable backend with a stable diagnostic naming the requested
  backend and current target before artifact output. Do not advertise or call a
  support-repository installer until Gate 16 implements that workflow.
- Print `async   compio · experimental` in build metadata only when async
  execution is linked. A synchronous source neither starts nor links Compio.

## Performance invariants

No backend string parsing, trait-object dispatch, channel hop, allocation, or
lock occurs per poll. Release dead-code elimination removes the entire async
adapter and Compio dependency from a synchronous control artifact.

## Completion proof

Public CLI/native tests cover precedence, `auto`, Compio, unavailable backends,
targets, no-artifact failures, output rendering, static symbols, sync/async size
deltas, and profiler/allocation evidence for the actual adapter hot path.

## Closure evidence

The concrete `CompioBackend` drives the exact backend-neutral facade root through
Compio's local runtime and backend waker; focused tests prove a pending/woken root
completes in two direct polls with zero Rimera wrapper allocations, while the
steady-state allocation counter remains zero across 1,000 warmed adapter polls.
Public native builds select Compio for reachable `rimera.async_runtime` roots,
execute the source-level root, expose the static
`rimera_async_backend_select_compio`/Compio symbols, and contain neither Monoio
nor Tokio symbols. An equivalent synchronous control links none of the async or
backend symbols even when `compio` is explicitly requested, and the release
async-vs-sync size delta remains within the fixed 524,288-byte Slice 1 budget.

`gate10_slice10_cli.rs` exercises the shipped CLI rather than only parser helpers:
`--async compio` overrides a project `async = "tokio"`, `--async auto` resolves
to Compio for async-reachable source, configured Tokio fails with stable
`RIM-ASYNC-001` target/backend text before artifact publication, and synchronous
source does not render the experimental async metadata row. Monoio and Tokio are
recognized selections but remain unavailable adapters rather than alternate
Python semantic implementations.

The closure audit extends the allocation measurement to the actual Compio
backend waker/scheduler path: 1,000 warmed wake/poll cycles allocate zero times.
The CLI test executes the generated auto/explicit/synchronous artifacts and
checks their exit status, stdout, and stderr. It also checks Monoio, unknown
backend values, and unsupported targets publish no artifact. Concurrent native
builds now use distinct manifest temporary filenames within a process; the
workspace's parallel native suite exercises publication without the previous
PID-only rename race.

## DONE BY CHATGPT
