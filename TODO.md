# Rimera Lite compatibility resume board

This is the short, operational checklist for continuing Rimera Lite without
repeating completed work.  The authoritative details live in
[`spec/compatibility.md`](spec/compatibility.md); architecture and sequencing
remain governed by [`spec/architecture.md`](spec/architecture.md) and
[`spec/rebuild.md`](spec/rebuild.md).

## How to resume work

1. Read this file, `AGENTS.md`, and `spec/compatibility.md` before editing.
2. Start at **Active gate** unless the requested work exposes an earlier
   correctness regression.
3. Reuse the existing runtime objects, ABI, MIR, and compiler stages.  Do not
   rebuild a capability already checked below or create a parallel model.
4. A box becomes checked only after the feature works through source -> HIR ->
   verified MIR -> Cranelift -> Rust runtime -> GC/lifetime proof -> public
   native test.  Runtime scaffolding alone does not earn a check.
5. When a box changes, update this board and `spec/compatibility.md` in the same
   change and cite the proof.

## Completed gates

- [x] **Compact native builtin type kernel**
  - Dedicated, lazily materialized and GC-rooted types for `NoneType`, `bool`,
    `int`, `str`, list, tuple, dict, set, range, functions, builtin functions,
    iterators, cells, and tracebacks.
  - Exact `type(value)` identity, including `bool -> int -> object` and concrete
    exception types.
  - Generic-call-path `type`, `isinstance`, and `issubclass`, including nested
    tuple class-info and CPython-compatible failures.
  - Preserve both public 8 KiB heap-limit regressions and prove lazy startup.
  - Proven by `ffi::tests::type_kernel_is_lazy_exact_and_gc_rooted`,
    `ffi::tests::builtin_type_calls_follow_generic_call_and_class_info_rules`,
    and the public `builtin_type_kernel_*` native-pipeline tests.

- [x] **Generic class and instance allocation**
  - Module-scope `class Name: pass`, `type(name, bases, namespace)`, and
    zero-argument class calls use the same native class constructor.
  - User classes have stable `type` identity, `[class, object]` MROs, distinct
    instance dictionaries, generic `isinstance`/`issubclass`, and traced
    class-instance cycles.
  - Proven by `ffi::tests::user_classes_have_stable_identity_instances_and_mro`,
    `ffi::tests::user_class_instance_cycles_are_collectible`,
    `ffi::tests::class_constructor_abi_creates_a_rooted_native_type`, and the
    `empty_classes_and_dynamic_type_construction_use_the_native_object_kernel`
    public native-pipeline test.

- [x] **Attribute read, write, and delete; bound methods**
  - Module-scope class assignments and function definitions populate ordered,
    traceable class namespaces; instances own independent ordered dictionaries.
  - Native attribute read/write/delete follows instance then current-MRO class
    lookup, and class functions become traceable bound methods through the
    generic call ABI.
  - Proven by `ffi::tests::attributes_and_bound_methods_are_traced_and_collectible`,
    `attributes_and_bound_methods_follow_the_native_object_protocol`, and
    `missing_attributes_render_cpython_shaped_failures`.

- [x] **Inheritance, subtype behavior, and C3 MRO**
  - User classes support ordered single and multiple inheritance, C3
    linearization, transitive subtype checks, inherited attributes and bound
    methods, and explicit two-argument `super` through the generic call path.
  - Duplicate bases, invalid dynamic bases, inconsistent hierarchies, builtin
    storage bases, and deferred `super` forms have tested failure boundaries.
  - Proven by `ffi::tests::user_inheritance_uses_c3_and_super_values_trace_the_hierarchy`,
    `ffi::tests::c3_rejects_duplicate_and_inconsistent_user_bases`,
    `mir::tests::class_construction_roots_ordered_bases_and_namespace`, and the
    public `inheritance_c3_and_explicit_super_match_cpython` differential test.

- [x] **Descriptors, slots, `__class__` cells, and zero-argument `super()`**
  - Descriptor-aware lookup, native `property`/`staticmethod`/`classmethod`,
    custom descriptor calls, inherited traced `__slots__` storage, shared class
    cells, and zero-argument `super()` all run through generic calls and native
    closures. Proven by `descriptor_decorators_follow_the_native_object_protocol`,
    `slots_use_member_descriptors_and_preserve_inherited_layouts`,
    `zero_argument_super_uses_the_native_class_closure_cell`, and
    `ffi::tests::slots_install_traced_member_descriptors_and_inherit_storage`.

- [x] **Full class/metaclass construction protocol and class decorators**
  - Native `__set_name__`, `__init_subclass__`, and bottom-up class decorators
    now use the generic call path; public proof is
    `descriptor_set_name_hooks_run_during_native_class_construction`,
    `init_subclass_hooks_run_after_native_type_creation`, and
    `class_decorators_use_the_generic_native_call_path`.
  - Most-derived metaclass selection, managed `__prepare__`, custom
    `__new__`/`__init__`, `__mro_entries__`, metaclass data-descriptor
    precedence, and custom attribute hooks now use generic native calls.
    Proven by the public `user_metaclass_*`, `implicit_metaclass_selection_*`,
    `non_type_bases_*`, `metaclass_data_descriptors_*`, and
    `custom_attribute_hooks_*` tests.
  - Class suites are hidden native `ClassBody` functions over the prepared
    namespace, with namespace-first global/builtin fallback, native closure
    capture, and class-cell publication. Source class-body expressions, conditionals,
    `while`/`for` loops with loop control, local reads, name-target augmented
    assignment, item/attribute mutation, deletion, `raise`, ordinary
    `try`/`except`/`else`/`finally` (including loop-exit cleanup), nested
    classes, and methods in existing class control-flow blocks now lower
    through native namespace operations; proof: `class_body_*` native-pipeline
    tests and `builtin_storage_subclass_payloads_use_native_collection_operations`.
  - `__bases__` now plans every descendant C3 MRO before committing hierarchy
    mutation and version invalidation. Supported list/tuple/dict/set subclasses
    retain a traced native payload while preserving the user class identity.
  - Custom `__prepare__` mappings use native item-protocol calls during class
    execution; custom `__new__` implementations receive that mapping and may
    convert it before delegating to `type`. Class keywords are forwarded to
    `__prepare__`, `__new__`, and `__init__` in source order. Proven by
    `prepared_mapping_objects_flow_through_native_class_body_and_type_creation`
    and `class_keywords_reach_prepare_and_metaclass_constructor_in_source_order`.

- [x] **Universal dunder and object protocol dispatch**
  - Shared `RBinaryOperator`/`RCompareOperator` contracts, in-place and item
    deletion ABI operations, strict-subclass reflected priority, native
    identity comparisons, hashing, display/formatting, containment and
    sequence/reverse-iteration fallbacks are proven by
    `universal_protocol_dispatch_uses_native_inplace_items_identity_and_builtins`.

- [x] **Native builtin values, collections, slicing, and builtin namespace**
  - Extend the established generic protocol path with the remaining managed
    builtin families and their public native proofs.
  - [x] Ordered hash-bucket lookup for native dictionaries and sets, live
    keys/values/items views, byte-oriented memoryview metadata and methods,
    bytearray-backed writable views with lifecycle export release, and the numeric/iterator helper subset
    are exercised by `bytes_complex_and_slice_literals_reach_the_native_runtime`.
  - [x] Exact mixed numeric equality/hashing, callback-mutation-safe generic
    dictionary/set lookup and removal, stored-hash native dict union, and
    mapping/iterable `|=` are proven against CPython 3.12.11 by
    `gate3_numeric_hash_and_collision_semantics_match_cpython_312`.
  - [x] Core `int`/`float`/`complex` parser and conversion parity, raw
    memoryview-to-bytes conversion, and builtin-storage subclass construction
    (including generic dict keys, mutable custom `__init__`, native
    `super().__init__`, inherited storage methods, and GC-safe payload handoff)
    are proven by `gate3_constructor_and_builtin_subclass_semantics_match_cpython_312`.
  - [x] CPython-shaped float/complex/string representation, Unicode 15.0
    printability, recursive dictionary-view repr protection, `ascii()`, and the
    supported int/float/complex/string format mini-language are proven by
    `gate3_repr_ascii_and_format_semantics_match_cpython_312`.
  - [x] Correctly rounded native float `round()` semantics and coordinated
    native/mixed float `divmod()` quotient/remainder behavior are proven against
    CPython 3.12.11 by `gate3_round_and_float_divmod_semantics_match_cpython_312`.
  - [x] Reverse iteration across builtin sequences/dicts/views plus huge-range
    len/truth/index/reverse behavior, range attributes, count/index methods, and
    canonical equality/hash are proven by
    `gate3_reversed_and_range_semantics_match_cpython_312`; native range slicing
    remains proven separately below the Gate 4 source-syntax boundary.
  - [x] Slice attributes/`indices()` and the remaining native-exporter memoryview
    cast/equality/hash/raw-conversion parity are proven by
    `gate3_slice_and_memoryview_remaining_semantics_match_cpython_312`.
    Python-level PEP 688 user buffer providers remain outside the Gate 3 claim and
    are now implemented by Gate 7 Slice 8 over this same memoryview core.
  - [x] Live dictionary views now finish reverse iteration, recursive repr,
    `.mapping`, set-like keys/items behavior, identity-style values equality,
    live mutation, tracing, and iterator invalidation; proven by
    `gate3_dictionary_view_finishing_matches_cpython_312`.
  - [x] Gate 3 managed-size accounting now charges retained list/hash-table
    capacity, ordered-entry/bucket/index storage, builtin byte/buffer metadata,
    and mutation growth while preserving the public low-heap GC regressions.
  - [x] The string domain is explicitly bounded to Unicode scalar values for
    Gate 3; supported `chr`/`ord` edges match CPython while lone surrogates fail
    with a structured, documented boundary owned by the final Unicode corpus.
  - [x] The 57-name synchronous builtin/type namespace is audited as lazy,
    first-class, aliasable, and rebindable; `Ellipsis` and `__debug__` now have
    explicit ownership without leaking internal runtime type names.
  - [x] Small Python-visible float/complex surfaces now use normal managed
    attribute lookup and bound calls (`real`, `imag`, `conjugate`, float
    `is_integer`, `as_integer_ratio`, and `hex`); class-level `float.fromhex`
    remains outside the Gate 3 claim and is now implemented by Gate 7 Slice 7
    through ordinary type attribute lookup.
  - [x] Gate 3's compact regression matrix now explicitly covers the three set
    update methods, middle empty-slice insertion for list/bytearray, and
    negative-step slice deletion while reusing the existing focused public
    differentials for the other high-risk Slice 22 cases.
  - [x] Final Gate 3 acceptance is green: CPython 3.12.11 is the differential
    oracle, the public native pipeline passes 111/111 tests, the runtime passes
    46/46 tests, low-heap/GC and unsupported-no-artifact proofs pass, and
    `scripts/verify_release.sh` produces a 485,064-byte release `hello` with no
    forbidden CPython/generated-C/`setjmp`/`longjmp` symbols.

## Proven native foundation — do not rebuild

- [x] Independent Rust workspace with four owned boundaries: ABI, runtime,
  compiler, and CLI.
- [x] Direct Python-source -> HIR/MIR -> Cranelift object -> Rust-runtime
  executable path, with no generated C, CPython embedding, or bytecode VM.
- [x] ABI v1 `RValue`, `RStatus`, opaque generational handles, and root frames.
- [x] Precise non-moving tracing GC with iterative marking, cycle collection,
  exact safepoint roots, temporary/context roots, adaptive byte thresholds,
  generation retirement, and optional heap limits.
- [x] Multi-function verified MIR, generic native call ABI, and large module
  initializer outlining.
- [x] Project-local `.rimera` cache, debug IR dump, `pyproject.toml`
  `[tool.rimera]` configuration, and build-and-run CLI flow.

Foundation evidence: [`spec/foundation.md`](spec/foundation.md),
[`crates/rimera-runtime/src/ffi.rs`](crates/rimera-runtime/src/ffi.rs), and
[`crates/rimera-compiler/tests/native_pipeline.rs`](crates/rimera-compiler/tests/native_pipeline.rs).

## Proven Python slices — preserve and extend

- [x] Literals, locals, branches, `while`, native printing, arbitrary-size
  integers, UTF-8 strings, and Python floor arithmetic for the tested subset.
- [x] Native functions, recursion, lambdas, positional-only and keyword-only
  parameters, defaults, `*args`, `**kwargs`, and supported binding failures.
- [x] Compile-time locals, module globals, builtin fallback, `global`,
  `nonlocal`, mutable closure cells, and unbound-name failures.
- [x] Structured native exceptions: typed/tuple handlers, bindings, bare
  reraising, cause/context/suppression, `else`, `finally`, cleanup completion,
  tracebacks, exception groups, and `except*` for the tested subset.
- [x] Lists, tuples, ranges, insertion-ordered generic-hash dictionaries and sets.
- [x] Native range/list/tuple/string/dict/set iteration plus user-defined
  `__iter__`/`__next__` protocol calls and `for` control flow.
- [x] Flat exact assignment unpacking and one final starred target.
- [x] Selected primitive arithmetic, comparison, sequence, indexing,
  assignment (including supported name-target augmented assignment and name
  deletion), equality, repetition, membership, true division, exponentiation,
  shifts, and bitwise behavior.
- [x] Managed float literals and mixed integer/float arithmetic for the
  implemented operator family; generic unary (`__pos__`, `__neg__`, and
  `__invert__`), binary/reflected, comparison,
  truth, length, item, containment, call, and iteration dunder paths with
  rooted `NotImplemented`.
- [x] Native byte, complex, and slice literals through syntax, HIR, MIR,
  Cranelift, ABI constructors, tracing GC, and a public CPython differential;
  bytearray slice mutation, frozenset construction, and native-exporter
  memoryview construction are covered by the same runtime family.
- [x] Generic-call builtins `getattr`, `setattr`, `delattr`, `hasattr`, and
  `callable` for the supported native attribute and callable model.
- [x] Generic type calls for supported `bool`, `int`, and `str` construction.

Public proof is concentrated in
[`crates/rimera-compiler/tests/native_pipeline.rs`](crates/rimera-compiler/tests/native_pipeline.rs).
These checks describe proven slices, not complete Python categories.

## Completed Gate 4

- [x] **Gate 4 — Unpacking, comprehensions, expanded calls, and remaining synchronous syntax**
  - Recursive/chained/starred assignment targets now work in module, function,
    loop, comprehension, match-body, and compiled class-body execution; generic
    `for`/dictionary unpacking and call-site `*iterable`/`**mapping` preserve
    CPython 3.12.11 order and failure behavior.
  - List/set/dictionary comprehensions, executable generator expressions,
    boolean short-circuiting, chained comparisons, slices, augmented assignment,
    deletion/assertions, walrus expressions, f-strings, annotations, and Python
    3.12 structural pattern matching execute through owned HIR/MIR/Cranelift and
    the Rust runtime with precise GC proofs.
  - Final acceptance is green: 3 ABI + 8 CLI + 37 compiler-lib + 145 public
    native-pipeline + 57 runtime tests, zero ignored tests, warning-denied clippy,
    doc tests, release build, and `git diff --check`. The release `hello` is
    485,144 bytes under the 512 KiB budget and passes the forbidden-symbol scan.

## Completed Gate 6

- [x] **Gate 6 — Native synchronous generators**
  - General source `yield`, yield-expression resume values, lazy construction,
    `__iter__`/`__next__`, `send`, `throw`, `close`, `GeneratorExit`, PEP 479,
    suspension through handlers/`finally`, and generic `yield from` delegation
    use the existing managed generator object and one native resume ABI.
  - Delegation covers native generators, generator expressions, builtin
    iterators, and user iterator/delegate objects with traced delegate and
    pending-exception state; constrained-heap public CPython differentials and
    runtime GC/failure-atomicity tests are green.
  - Final acceptance is green: 3 ABI + 8 CLI + 40 compiler + 159 public native
    pipeline + 59 runtime tests = 269 passed, zero failed/ignored; warning-denied
    clippy/doc/diff checks pass. `scripts/verify_release.sh` produces a
    485,176-byte `hello` under the 512 KiB ceiling with a clean forbidden-symbol
    scan.

## Recently completed compatibility gates

- [x] **Gate 5 — functions, LEGB, and structured exceptions closed**
  - All eight slices are complete on the single binder/cell/exception/traceback/
    cleanup model. The injected-exception generator-resume liveness prerequisite
    is repaired in MIR persistence and the final Gate 5 composition/audit is
    green.
- [x] **Gate 7 — reflection and introspection closed**
  - All ten slices are complete. Slice 9 proves mutation invalidation,
    cross-family composition, GC, and low-heap atomicity; Slice 10 closes the
    owner/release/forbidden-symbol audit and promotes reflection to
    `Implemented — conformance audit pending`.
- [x] **Gate 8 — synchronous context managers closed**
  - All eight slices are complete on compiler-owned cleanup CFG. Public proof
    covers descriptor-bound enter/exit, every supported target and completion,
    exact exceptional triples, suppression/replacement/chaining, suspended and
    abandoned generators, cross-feature GC, constrained heaps, and native-only
    artifacts.

- [x] **Gate 9 Slice 1 — ownership, canonical identity, state machine, and oracle matrix**
  - Extend the existing narrow managed module registry into the one
    deterministic module identity/state model specified by
    [`spec/gates/9/README.md`](spec/gates/9/README.md).
  - Do not claim stdlib or package compatibility from the Gate 7 module shells;
    this slice owns canonical module identities, graph/state ownership, and the
    proof matrix that later Gate 9 slices execute.

## Completed Gate 9 discovery and native-module slices

- [x] **Gate 9 Slice 2 — deterministic project discovery, graph edges, and diagnostics**
  - Discover reachable project-owned source modules without executing them or
    consulting ambient Python/venv state.
  - Preserve canonical graph order, import source spans, hashes, and stable
    no-artifact diagnostics as specified by
    [`spec/gates/9/slice-2.md`](spec/gates/9/slice-2.md).

- [x] **Gate 9 Slice 3 — per-module MIR/object emission, linking, and native initialization**
  - Reachable source modules are independently analyzed and lowered, emitted as
    deterministic native objects, and linked through the documented native
    initializer ABI. Public chain/diamond proof covers one-time initialization,
    isolated defining globals, CPython-matched output, and native-only symbols.

## Completed Gate 9 cache and ordinary-import slices

- [x] **Gate 9 Slice 4 — cached identity, initialization cycles, and failure rollback**
  - The authoritative cache now preserves identity through direct/indirect
    cycles, exposes partial modules, rolls failed initialization back for retry,
    retains successful dependencies, and survives constrained heaps.
- [x] **Gate 9 Slice 5 — plain and dotted imports, aliases, and binding order**
  - Dotted prefixes initialize in order, ready children publish on parents,
    explicit aliases bind leaves, unaliased imports bind the top-level module,
    and imports use normal bindings in module/function/class/control/generator
    scopes.

## Active gate

- [ ] **Gate 11 Slice 3 — `eval` globals/locals, builtins injection, and closure reads**


## Completed Gate 11 foundation slices

- [x] **Gate 11 Slice 1 — modes, code-object contract, capabilities, and oracle matrix**
  - `exec`/`eval`/`single`, context/native-unit ownership, explicit
    `dynamic_compilation` CLI/project capability, CPython 3.12.11 oracle
    behavior, and enabled/disabled artifact symbol reachability are frozen.
- [x] **Gate 11 Slice 2 — `compile` sources, flags, diagnostics, and code metadata**
  - Capability-gated native `compile()` now publishes immutable managed code
    only after the normal parser/sema/HIR/verified-MIR/Cranelift path succeeds.
    Supported source/filename shapes, `flags=0`, `dont_inherit`, optimization,
    syntax metadata, low-heap behavior, and native-only proof are documented.

## Completed Gate 9 package and cache-visibility slices

- [x] **Gate 9 Slice 6 — `from` imports, `__all__`, star imports, and missing names**
  - Named/aliased/parenthesized imports, statically resolved submodule fallback,
    sequence-indexed dynamic `__all__`, public-name fallback, partial bindings,
    managed failures, and illegal-scope no-artifact diagnostics now pass public
    native differentials under a constrained heap.
- [x] **Gate 9 Slice 7 — regular packages, `__init__`, relative imports, and metadata**
  - Regular packages initialize through the authoritative module state machine;
    level-one/level-two relative imports, parent ordering, reentrant cycles, and
    managed module/loader/spec metadata match CPython at the public native edge.
- [x] **Gate 9 Slice 8 — namespace packages, search roots, and parent publication**
  - Ordered `module_roots` merge namespace portions through one managed module
    identity, regular packages take Python-compatible precedence, children
    publish on their parents, and namespace metadata survives constrained-heap
    native differential proof.
- [x] **Gate 9 Slice 9 — authoritative `sys.modules`, `__import__`, and cache mutation**
  - `sys.modules` is the live context-owned cache used by imports; generic
    `__import__` covers direct/fromlist/relative-level behavior and invalid
    arguments, supported cache replacement/deletion/`None` entries match
    CPython, and managed-growth preflight keeps cache mutations atomic when the
    heap limit rejects an insertion.

## Completed Gate 9 final slices

- [x] **Gate 9 Slice 10 — import hooks, reload, invalidation, and reentrant loading**
  - Managed `builtins.__import__` hooks, recursive callbacks, reload identity,
    failure handling, and invalidation remain on the authoritative module state
    machine and match CPython in the public differential.
- [x] **Gate 9 Slice 11 — locked dependencies, package data, capabilities, and cache manifests**
  - Locked resource hashes and deterministic build manifests are reproducible;
    stale resource inputs fail before artifact publication.
- [x] **Gate 9 Slice 12 — cross-feature composition, reentrancy, and constrained-heap stress**
  - Cyclic imports, hooks, generator/context cleanup, repeated reload, and
    `sys.modules` identity match CPython under a 64 KiB managed heap.
- [x] **Gate 9 Slice 13 — reproducibility, final audit, documentation, and gate closure**
  - Final workspace/native/runtime/doc, warning-denied Clippy, formatting/diff,
    reproducibility, release-size, and forbidden-symbol boundaries are green.
  - Gate 9 imports/modules/packages is closed as `Implemented — conformance audit pending`.

## Completed Gate 10 foundation slices

- [x] **Gate 10 Slice 1 — ownership, oracle matrix, benchmark harness, and fixed budgets**
  - CPython 3.12.11 protocol/syntax oracles, target baseline distributions,
    structural invariants, and fixed measured budgets are frozen and verified.
- [x] **Gate 10 Slice 2 — async syntax, HIR/MIR suspension, resume, and injection contracts**
  - `async def`/`await` reach owned syntax/HIR and explicit verified coroutine
    suspension; invalid async placements match CPython and emit no artifact.
- [x] **Gate 10 Slice 3 — coroutine objects, lazy calls, direct `await`, and lifecycle**
  - Lazy native coroutine calls, direct nested await, completion/failure/reuse,
    reflection, warnings, native-only artifacts, and constrained-heap behavior
    match CPython through the public protocol fixture.

## Completed Gate 10 executor/protocol slices

- [x] **Gate 10 Slice 4 — executor facade, task identity, local scheduling, and completions**
- [x] **Gate 10 Slice 5 — timers, wakeups, cancellation delivery, and buffer ownership**
- [x] **Gate 10 Slice 6 — generic `__await__`, delegation, injected failures, and cleanup**
- [x] **Gate 10 Slice 7 — async iteration, `async for`, and async comprehensions**
  - Public/native and runtime acceptance covers the one-root ABI bridge, zero
    synchronous async/backend reachability, deterministic wakes/timers,
    cancellation and buffer lifetime, generic awaitable delegation, async
    iteration/comprehensions/generator expressions, and constrained heaps.
- [x] **Gate 10 Slice 8 — async generators, `asend`, `athrow`, `aclose`, and finalization**
  - Public CPython 3.12 differentials now cover lazy async-generator objects,
    complete operation/reuse/overlap behavior, await/yield interleaving,
    exception injection, cancellation-style cleanup, reflection/tracebacks,
    awaited close cleanup, ignored `GeneratorExit`, cyclic abandonment and
    shutdown finalization, a 5,000-value stream under a 96 KiB managed heap,
    invalid non-empty return diagnostics, and native-only artifacts.
- [x] **Gate 10 Slice 9 — `async with`, partial entry, suppression, and suspended cleanup**
  - Public CPython 3.12 proof covers type-level special lookup, multiple/partial
    acquisition, target failures, every control transfer, suppression and
    replacement, suspending/failing entry and exit, cancellation through
    suspended cleanup, mixed sync/async managers, traceback frames, self-cycles,
    and a 128 KiB managed-heap differential.
  - Closure audit adds captured descriptors, custom truth/failure suppression,
    exact missing-method errors, and cancellation during partial entry and exit.
- [x] **Gate 10 Slice 10 — Compio adapter, static linking, CLI selection, and diagnostics**
  - The concrete local Compio adapter directly drives one facade root with zero
    Rimera wrapper/steady-poll allocation, public async artifacts statically
    select Compio while sync controls retain zero backend symbols, and CLI/native
    proof covers auto/explicit/project precedence, stable unavailable-backend
    no-artifact diagnostics, metadata rendering, and the fixed release-size delta.
  - Allocation proof includes 1,000 real Compio wake/poll cycles; CLI proof runs
    produced artifacts and rejects unknown backend values and unsupported targets.
- [x] **Gate 10 Slice 11 — cross-feature composition, GC, cancellation, and performance stress**
  - Repeated CPython 3.12.11 public differentials compose the entire async language
    surface under normal and 512 KiB heaps; race/model stress covers wake,
    completion, cancellation, timers, shutdown/drop, re-entry, stale handles, and
    many-root progress; all frozen release performance budgets pass.
- [x] **Gate 10 Slice 12 — reproducibility, final audit, documentation, and gate closure**
  - Full workspace/native/runtime/doc tests, warning-denied Clippy, formatting,
    ABI layout, release-size/forbidden-symbol checks, frozen oracle evidence,
    refreshed CPython/native distributions, and clean-cache reproducibility are
    green. Gate 10 async/await/protocols is closed as `Implemented — conformance
    audit pending` on macOS ARM64 with Compio; `asyncio` remains outside the gate.
- [x] **Gate 10 Slice 13 — proven-ready AOT collapse and CPython performance lead**
  - The checked-in release verifier now requires Rimera to beat CPython 3.12.11
    on every comparable p50 speed/throughput row. Same-source create+immediate-
    close is 2.34 ns p50 versus 63.94 ns CPython (27.35x faster), the pure-ready
    million-await workload is 30.90x faster, and rebinding/low-heap/final-local
    differentials prove the AOT eliminations retain the full semantic fallback.

## Queued compatibility gates

These remain unchecked until their own end-to-end evidence lands. Gate 11 Slice 3
is the sole active compatibility slice; later post-core areas remain separate
promotions and must not be implied complete.
- [ ] **Gate 11 — capability-governed `compile`, `eval`, `exec`, and runtime
  native compilation**, split into eight queued slices in
  [`spec/gates/11/README.md`](spec/gates/11/README.md).
- [ ] **Gate 12 — weak references, finalizers, and resurrection semantics**,
  split into eight queued slices in
  [`spec/gates/12/README.md`](spec/gates/12/README.md).
- [ ] **Gate 13 — Python 3.12 language/runtime conformance corpus**, split into
  eight queued slices in [`spec/gates/13/README.md`](spec/gates/13/README.md).
- [ ] **Gate 14 — Python 3.12 standard library**, split into eight queued slices
  in [`spec/gates/14/README.md`](spec/gates/14/README.md).
- [ ] **Gate 15 — native extensions, platform ABI, and foreign
  interoperability**, split into eight queued slices in
  [`spec/gates/15/README.md`](spec/gates/15/README.md).
- [ ] **Gate 16 — packaging and PyPI ecosystem compatibility**, split into
  eight queued slices in
  [`spec/gates/16/README.md`](spec/gates/16/README.md).
- [ ] **Gate 17 — Python 3.12 drop-in release qualification**, split into eight
  queued slices in [`spec/gates/17/README.md`](spec/gates/17/README.md).

## Permanent completion checks

- [ ] General Python 3.12 compatibility claim — intentionally unchecked until
  the relevant language, stdlib, platform, and package corpora prove it.
- [ ] Qualified Python 3.12 drop-in guarantee — intentionally unchecked until
  Gate 17 closes for a published target, capability, stdlib, and extension-ABI
  matrix with no partial required surface.
- [x] Unsupported behavior fails with a stable source diagnostic and emits no
  fallback artifact for the currently tested boundaries.
- [x] Tested native artifacts exclude CPython, generated-C, `setjmp`, and
  `longjmp` symbols.
