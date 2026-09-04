# Gate 7 Slice 2 — Namespace Views: `globals`, `locals`, `vars`, and `dir`

## Goal

Expose namespaces through managed values with explicit Python 3.12 snapshot,
live, ordering, and mutation behavior.

## Integrated work

- Implement `globals()` over the context-owned module namespace and define its
  live identity and write-through behavior through ordinary dictionary paths.
- Implement `locals()` for module, function, class-body, comprehension, and
  suspended-generator contexts with CPython-compatible observable semantics for
  each supported scope.
- Implement `vars()` with and without an argument, including instance/class/
  module dictionaries, missing-dictionary failures, and descriptor interaction.
- Implement `dir()` default and object forms using supported instance, class,
  base, metaclass, and custom `__dir__` sources with sorting and validation.
- Trace namespace/view owners without retaining dead activations accidentally;
  mutation and allocation failures must leave source namespaces valid.

## Native ownership and ABI path

- `globals`, `locals`, `vars`, and `dir` are lazily published managed builtin
  functions. Compiled source resolves and invokes them through the same generic
  global lookup and `rimera_call` path as every other builtin; there are no
  compiler-only intrinsics for these names.
- Function scopes continue to execute from the authoritative compiled `Cell`
  bindings. Lowering emits `ReflectionScopeConfigure` once for a native scope and
  `ReflectionLocalRegister` for the same cells used by reads/writes. Cranelift
  lowers those operations to the opaque Rust ABI helpers
  `rimera_reflection_scope_configure` and `rimera_reflection_local_register`.
  No duplicate frame-local storage or runtime MIR evaluator exists.
- Class bodies configure their already-authoritative prepared namespace directly;
  module calls use the context-owned globals dictionary.
- Function activations own one lazily allocated managed locals dictionary. Every
  `locals()`/zero-arg `vars()` call refreshes compiled binding keys from the real
  cells while preserving user-inserted keys that do not correspond to compiled
  locals. The same dictionary identity is returned for the life of the
  activation and an independently retained dictionary survives activation exit.
- Local observation order is supplied by semantic analysis: parameters in
  declaration order followed by the first source binding of each remaining local.
  Reassignment never reorders the key.
- Python 3.12 list/set/dict comprehensions use an inlined-observation overlay over
  their nearest non-comprehension owner. The hidden native function remains an
  implementation detail: registered iteration cells are temporarily visible in
  the owner mapping and are restored/removed when the hidden activation exits.
  Nested inlined comprehensions compose their overlays.
- Generator expressions are deliberately different: they retain their own
  suspended frame-style locals snapshot with `.0`, iteration locals, and only
  referenced free variables, matching CPython 3.12. Source generators use the
  same persistent activation mechanism for their ordinary function locals.
- A suspended generator copies its reflection cell registry and locals snapshot
  between the active-call record and the managed generator object on every
  resume/pop. Terminal completion drops the generator's internal snapshot/cell
  roots; independently retained dictionaries remain normal traced managed values.
- One-argument `vars()` is ordinary `__dict__` attribute access. Instances expose
  their live dictionary, classes expose their live mappingproxy, managed module
  objects expose their live module namespace, and objects without `__dict__`
  raise the CPython-shaped `TypeError` path. Slice 1's pulled-forward `inspect`
  and `weakref` shells therefore compose with the same namespace path rather
  than a module-specific reflection store.
- Zero-argument `dir()` sorts current local names. Default `dir(obj)` combines the
  supported instance/class/base/metaclass sources. A custom `__dir__` is resolved
  and called normally, its result is consumed through the generic iterator path,
  and the resulting managed list is sorted through ordinary list sort semantics;
  callback exceptions and invalid values are not reclassified by a special
  compiler path.

## GC and failure ownership

- Active-call reflection roots are part of the context root graph: configured
  namespaces, local cells, current snapshots, and temporary comprehension overlay
  state remain reachable exactly while the activation needs them.
- Generator reflection cells/snapshots are traced from the generator while
  suspended and are cleared from terminal generator state when no longer needed.
- Locals dictionaries and cycles are ordinary managed heap objects; retaining one
  keeps only its own reachable graph alive. Dead activation/snapshot cycles are
  reclaimable under the managed heap limit.
- Reflection allocations use the existing heap-limit/`MemoryError` machinery;
  an allocation failure cannot publish a half-configured namespace view or mutate
  the source namespace through a second storage path.

## Public CPython 3.12.11 proof

- `gate7_namespace_views.py` is a native-only differential covering:
  - live module `globals()` identity/write-through plus module `locals()`/`vars()`;
  - function locals ordering, same-dict refresh, retained user keys, and retained
    post-return snapshots;
  - live class namespace behavior and class/instance one-argument `vars()`;
  - no-`__dict__` failure;
  - default and custom `dir()` sorting/duplicates/error behavior;
  - nested list-comprehension owner overlays and no target leakage;
  - class-body comprehension observation;
  - source-generator snapshot identity across suspension;
  - generator-expression `.0`/loop/free-variable locals semantics.
- `gate7_namespace_views_gc.py` runs under a 96,000-byte managed heap limit and
  proves retained function/generator locals graphs survive collection while dead
  self-cyclic snapshots/activations are reclaimed.
- `gate7_import_foundation.py` additionally proves `vars(module)` observes the
  same live namespace used by module attribute get/set/delete, and repeated
  imports retain one managed module identity.
- Focused MIR proof `gate7_namespace_reflection_mir_declares_authoritative_scope_ownership`
  checks function/class/comprehension/genexpr scope configuration and local-cell
  registration. Focused sema proof
  `gate7_local_observation_order_preserves_parameters_then_first_bindings` pins
  CPython-visible local ordering.
- Runtime regression `gate7_reflection_call_heap_failure_is_memory_error_and_source_namespace_survives`
  forces a reflective allocation failure through ordinary `rimera_call`, proves
  managed `MemoryError` is preserved instead of becoming `TypeError`, and proves
  the observed module namespace remains valid before a successful retry.

## Completion proof

Public differentials cover scope kinds, ordering, aliases, mutations, custom
hooks, failures, completed activations, suspended generators, forced GC, and
heap limits. Focused Gate 7 proof is green with 10 tests total, and the shared
Slice 1–2 boundary is green under `cargo fmt --check`, warning-denied workspace
clippy, `cargo test --workspace` (279 passed, 0 failed/ignored, including doc
tests), and `git diff --check`.

## DONE BY CHATGPT
