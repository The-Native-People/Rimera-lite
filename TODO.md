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
    Python-level PEP 688 user buffer providers are explicitly owned by Gate 7.
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
    is explicitly deferred to Gate 7's builtin/reflection method-table audit.
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

## Active gate

- [ ] **Gate 4 — Unpacking, comprehensions, expanded calls, and remaining synchronous syntax**
  - Nested/starred assignment targets, `for` and dictionary unpacking, call-site
    `*iterable`/`**mapping`, comprehensions, generator-expression syntax, and the
    remaining source forms defined by `GATES.MD` are the sole active next work.
  - Gate 3 builtin values, conversions, collection protocols, and slicing are
    complete within their documented boundaries and must not be folded back
    into this gate.

## Queued compatibility gates

These remain unchecked until their own end-to-end evidence lands. Work them in
`GATES.MD` dependency order; do not skip ahead for framework-specific behavior.

- [ ] Functions/LEGB/structured-exception closure work.
- [ ] Generators and suspension.
- [ ] Reflection and introspection.
- [ ] Context managers.
- [ ] Imports, packages, modules, and `sys.modules`.
- [ ] Async/await and async protocols.
- [ ] Capability-governed `eval`, `exec`, and runtime native compilation.
- [ ] Weak references, finalizers, and resurrection semantics.
- [ ] Python 3.12 edge-semantics corpus.
- [ ] Pure-Python standard-library corpus.
- [ ] Native standard-library/platform bindings.
- [ ] Real PyPI package corpus and framework acceptance workloads.

## Permanent completion checks

- [ ] General Python 3.12 compatibility claim — intentionally unchecked until
  the relevant language, stdlib, platform, and package corpora prove it.
- [x] Unsupported behavior fails with a stable source diagnostic and emits no
  fallback artifact for the currently tested boundaries.
- [x] Tested native artifacts exclude CPython, generated-C, `setjmp`, and
  `longjmp` symbols.
