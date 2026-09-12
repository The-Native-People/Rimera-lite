# Implemented native foundation

## Delivered

- The repository has an independent Git boundary and a pinned Rust workspace.
- Four crates provide clear boundaries: the CLI, compiler pipeline, runtime,
  and ABI. Compiler stages are internal modules to avoid micro-crate overhead.
- ABI v1 provides 16-byte values, status returns, generational handles, and
  explicit root frames.
- The Rust runtime owns arbitrary-size integers, UTF-8 strings, runtime
  failures, and a precise non-moving tracing heap. It traces object graphs with
  an iterative worklist, rejects stale/exhausted handles, uses exact safepoint
  roots, and supports adaptive byte pressure plus an optional context limit.
- Verified SSA-style MIR lowers through Cranelift directly to arm64 Mach-O.
- The linker accepts object files and the Rust runtime archive; it never
  accepts C sources.
- The public compiler API and `rimera-lite build` compile the first Python subset.
- The execution kernel now includes managed type, function, cell, tuple,
  dictionary, exception, traceback, and exception-group objects. Compiled
  modules contain multiple native MIR functions using one generic call ABI.
- Symbol planning implements module globals, compile-time locals, shared
  closure cells, `global`, and `nonlocal`. `def` and lambda expressions share
  the full Python parameter binder, native function ABI, and GC root plan.
- Structured exceptions support typed and tuple handlers, handler bindings,
  bare reraising, causes, contexts, suppression, `else`, `finally`, cleanup
  across return/break/continue, native traceback propagation, and `except*`
  subgroup splitting/merging.
- Gate 8 completes Rimera-owned synchronous `with` syntax/HIR and verified
  cleanup MIR. Captured type/MRO special methods use generic calls; multiple
  managers, assignment targets, partial entry, every completion, exception
  triples/suppression/chaining, generator suspension and abandonment, and
  cross-feature GC share the existing exception, generator, and cleanup models.

## Proof

`crates/rimera-compiler/tests/native_pipeline.rs` proves:

- hand-built MIR keeps managed values and child graphs alive across forced
  collection;
- dead SSA values are reclaimed under a constrained heap while reachable
  values fail deterministically when the same limit is exceeded;
- Python locals, loops, branches, strings, big integers, and floor arithmetic;
- differential stdout/stderr/exit behavior against CPython 3.12;
- structured nonzero runtime failures;
- rejected source emits no artifact;
- final binaries contain no legacy Python or C-unwind symbols.
- recursive functions, lambdas, mutable closures, globals, exact unbound-name
  classes/messages, and CPython 3.12 call-binding failures;
- ordinary exception control flow, cleanup completion reasons, chained
  tracebacks, all-matched and partially matched exception groups, and handler
  failures that do not prevent later `except*` handlers from running;
- managed arguments, defaults, and closure cells surviving collection pressure
  during deep native recursion.
- ordered native outlining for large module initializers. The committed
  39,371-line compile-stress fixture lowers into verified module chunks,
  compiles without Cranelift function-size failures, produces CPython-matching
  output, and preserves a single source-accurate module traceback frame.
- synchronous context-manager lookup/call order, missing and failing methods,
  captured-exit mutation, all supported `as` target shapes, multiple and
  parenthesized acquisition, partial-entry unwind, return/break/continue,
  exact exception triples, suppression/replacement/chaining, nested `finally`,
  class-suite execution, generator send/throw/close/delegation/abandonment,
  reflection and buffer composition, reentrancy, cycles, constrained heaps,
  and native-only artifacts against CPython 3.12.11.

Runtime unit tests prove child tracing, self and multi-object cycle collection,
deep iterative marking, generation retirement, nested and temporary roots,
adaptive byte thresholds, heap limits, Python floor arithmetic, and
large-integer handles. MIR tests prove exact branch, loop, and shrinking
safepoint live sets.

The release `print("hello")` artifact measures 503,544 bytes on macOS arm64,
below the 512 KiB milestone budget. Release linking dead-strips unreachable
sections and strips debug metadata; its only dynamic dependency is
`/usr/lib/libSystem.B.dylib`.

## Next capability

Gate 9 Slice 1 closed the canonical graph and managed module-state ownership.
Gate 9 Slice 2 is the sole active compatibility slice and owns deterministic
project-source discovery, graph edges, hashes, spans, and diagnostics
that the later import/package slices execute. It must extend the narrow managed
module registry established by Gate 7 rather than introduce a parallel module
representation.
