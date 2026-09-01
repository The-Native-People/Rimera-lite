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
- The public compiler API and `rimera build` compile the first Python subset.
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

Runtime unit tests prove child tracing, self and multi-object cycle collection,
deep iterative marking, generation retirement, nested and temporary roots,
adaptive byte thresholds, heap limits, Python floor arithmetic, and
large-integer handles. MIR tests prove exact branch, loop, and shrinking
safepoint live sets.

The release `print("hello")` artifact measures 470,408 bytes on macOS arm64,
below the 512 KiB milestone budget. Release linking dead-strips unreachable
sections and strips debug metadata; its only dynamic dependency is
`/usr/lib/libSystem.B.dylib`.

## Next capability

The completed object-model gates add executable module-scope class bodies,
traceable class and instance dictionaries, native attribute read/write/delete,
bound compiled methods, user multiple inheritance, C3 MRO, subtype behavior,
and explicit two-argument `super` through the generic call ABI. The active
compatibility milestone is descriptor precedence, slots, `__class__` cells,
and zero-argument `super`; metaclass construction and operator dunder dispatch
follow in the order recorded in `TODO.md` and `spec/compatibility.md`. Every
gate must reuse the managed type objects, generic call ABI, explicit exception
edges, and tracing contracts established here rather than introduce a
parallel representation.
