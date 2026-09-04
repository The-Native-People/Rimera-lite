# Python 3.12 compatibility ledger

**Status:** authoritative implementation ledger.  CPython 3.12.11 is the
behavioral oracle.  This document records proven capability, known limits, and
the order in which compatibility work proceeds.

Rimera does not claim general Python 3.12 compatibility.  A category is
complete only when its supported semantics pass public-entry differential
tests, GC and MIR verification where relevant, failure-path tests, and final
artifact symbol checks.  Runtime scaffolding or a passing unit test for one
layer is not category completion.

Compatibility claims are layered. Gate 13 may earn Python 3.12 language and
runtime compatibility; Gate 14 adds the standard library; Gate 15 states the
exact native-extension and platform ABI tier; Gate 16 proves package-ecosystem
coverage; and Gate 17 alone may issue a drop-in release guarantee for a
published target and compatibility matrix. None of those claims implies
untested operating systems, undocumented CPython internals, or incompatible
native extensions.

## Current position

The native compiler foundation is established: Python source flows through
Rimera-owned syntax, HIR, verified MIR, Cranelift object emission, and Rust
runtime linking.  It does not generate C, embed CPython, execute Python
bytecode, or use host unwinding for Python exceptions.

The execution kernel supports a meaningful but still bounded single-module
subset. Functions, lexical scopes, structured exceptions, tracing GC, the
Gate 1 class/object model, Gate 2 generic protocol dispatch, Gate 3 managed
builtin values/collections, Gate 4 unpacking/comprehension/remaining synchronous
source forms, and Gate 6 native synchronous generators are proven through the
native pipeline. Gate 7 Slices 1–8 additionally prove the owned reflection
matrix, `globals`/`locals`/`vars`/`dir` namespace views, the narrow cached managed
`inspect`/`weakref` module-shell prerequisite, generic identity/type/attribute
reflection, managed function/closure/signature/code metadata, managed
exception/traceback/frame and generator metadata, audited class/type method
tables, Python 3.12 generic-declaration `__type_params__`, and Python-level PEP
688 buffer-provider dispatch over the Gate 3 memoryview core. Gate 5's remaining
function/LEGB/exception conformance slices, Gate 7 integration/final audit, the
general import/package system, standard library, and ecosystem compatibility are
not yet established.

| # | Compatibility area | Status | Proven scope |
|---:|---|---|---|
| 1 | Exceptions and errors | Substantial partial | Typed and tuple handlers, bindings, bare reraising, causes, context and suppression, `else`, `finally`, cleanup completion, native tracebacks, exception groups, and `except*` |
| 2 | Function calling | Substantial partial | Native functions, recursion, lambdas, positional-only and keyword-only parameters, defaults, `*args`, `**kwargs`, and binding failures |
| 3 | LEGB, scopes, and closures | Substantial partial | Compile-time locals, module globals, builtins fallback, `global`, `nonlocal`, mutable cells, and unbound-name errors |
| 4 | Core object model | Implemented — conformance audit pending | Lazy GC-rooted builtin types, compiled prepared-namespace class bodies, classes/instances, C3 MRO with atomic descendant `__bases__` planning, descriptor-aware attributes, supported slots, class cells, both supported `super` forms, metaclass selection/hooks, `__mro_entries__`, custom attribute hooks, traced list/tuple/dict/set subclass payloads, custom prepared mappings through item protocols, and source-ordered class-keyword forwarding |
| 5 | Core builtin types | Implemented — conformance audit pending | Gate 3 managed `float`, `complex`, `bytes`, `bytearray`, `frozenset`, `slice`, dictionary views, and native-exporter `memoryview`; generic-call constructors, exact type identity, tracing/managed-size accounting, conversion, mutation/slicing, representation/formatting, iteration, and generic hash/equality collections within the documented boundaries |
| 6 | Operators and dunder dispatch | Implemented — conformance audit pending | Shared ABI operator discriminants, direct/reflected/in-place dispatch including `@`, strict-subclass priority, rooted `NotImplemented`, identity comparisons, truth/length/hash/item/call/iteration/conversion/representation/formatting protocols, target-aware augmented assignment, and item deletion all reach the native pipeline. Exhaustive CPython edge-case parity remains a conformance audit. |
| 7 | Iterators | Implemented — conformance audit pending | Native range, list, tuple, string, dictionary, set, generator-foundation, validated user `__iter__`/`__next__`, legacy `__getitem__` sequence fallback, containment fallback, and reverse iteration through `__reversed__` or `__len__`/`__getitem__` all reach `for` and public native fixtures. |
| 8 | Generators | Implemented — conformance audit pending | Gate 4 generator expressions and Gate 6 source generators share one traced generator object/resume ABI. Source `yield`, resume values, lazy construction, `iter`/`next`/`send`/`throw`/`close`, return/`StopIteration.value`, GeneratorExit, PEP 479, suspension through handlers/finally, and generic nested `yield from` delegation are proven against CPython 3.12.11 under constrained heaps. Async generators/async iteration remain later async-gate work. |
| 9 | Classes, inheritance, and MRO | Implemented — conformance audit pending | Compiled prepared-namespace class bodies, single/multiple inheritance, C3, atomic descendant `__bases__` planning, metaclass selection/hooks, builtin-storage subclasses, decorators, and supported explicit/zero-argument `super` |
| 10 | Descriptors | Implemented — conformance audit pending | Data/non-data precedence across instances/classes/metaclasses, native property/static/class methods, custom `__get__`/`__set__`/`__delete__`, traced member descriptors, slots, and compiled class cells |
| 11 | Context managers | Not started | No `with` or async context protocol |
| 12 | Imports, modules, and `sys.modules` | Not started | The general loader/package/`sys.modules` system is not started. Gate 7 Slices 1–2 pull forward only `import name` / `import name as alias` for registered managed `inspect` and `weakref` module shells, with cached identity and live traced namespaces; this is infrastructure, not an import-system compatibility claim. |
| 13 | Builtins | Implemented — conformance audit pending | Gate 3 audits the complete 57-name synchronous builtin/type namespace owned by the finished core as lazy, first-class, aliasable, and rebindable, including constructors, numeric/collection/iteration helpers, predicates, sorting/reduction, formatting/representation, and supported attribute helpers. Imports, dynamic compilation, async helpers, broad reflection method tables, and stdlib-dependent behavior remain explicitly later-gate owned. |
| 14 | Comprehensions and unpacking | Implemented — conformance audit pending | Recursive exact/starred/chained targets across normal and class-body execution, `for`/comprehension destructuring, dictionary unpacking, expanded calls, list/set/dict comprehensions, and executable generator expressions with CPython 3.12.11 differential, GC, failure-atomicity, and release-artifact proof |
| 15 | Reflection and introspection | Small partial | Gate 7 Slices 1–8 now prove the single-owner reflection matrix; native `globals`, `locals`, `vars`, and `dir` namespace semantics; narrow managed module-shell composition; generic identity/type/attribute reflection; managed function/code/cell, exception/traceback/frame, and generator metadata; audited class `__dict__`/`__bases__`/`__mro__`/`__name__`/`__qualname__`/`__module__` behavior; normal class-level `float.fromhex`; Python 3.12 `T`/`*Ts`/`**P` function/class/type-alias parameters with managed `__type_params__`; and Python-level PEP 688 `__buffer__`/`__release_buffer__` providers with shared traced leases. Retained metadata/provider graphs survive forced GC, final buffer release is exactly-once across derived views/failure/cycles, and low-heap publication/construction is atomic. Lazy PEP 695 bounds/constraints remain a stable `RIM-CAP-G7-03` boundary; mutation/composition integration and final audit remain Slices 9–10. |
| 16 | Async and await | Not started | Syntax and execution are deferred |
| 17 | `eval`, `exec`, and runtime compilation | Not started | No dynamic-code pipeline |
| 18 | Weak references and finalizers | Phase reserved only | GC has a lifecycle phase boundary but exposes neither behavior |
| 19 | Python 3.12 edge semantics | Narrow partial | Selected floor arithmetic, call binding, exception cleanup, and traceback behavior |
| 20 | Pure-Python standard-library corpus | Not started | No corpus claim |
| 21 | Native stdlib and platform bindings | Not started | No native module corpus claim |
| 22 | Real PyPI package corpus | Not started | No package compatibility claim |

## Foundation already earned

- ABI v1: 16-byte `RValue`, explicit `RStatus`, opaque generational handles,
  and stack-owned root frames.
- Precise non-moving tracing GC: iterative graph marking, cycle collection,
  exact safepoint roots, temporary and context roots, adaptive managed-byte
  thresholds, generation retirement, and optional heap limits.
- Multi-function verified MIR and direct Cranelift Mach-O emission.
- Generic native function-call ABI and CPython-shaped argument binding for the
  supported call shapes.
- Explicit exception CFG edges, managed exception state, traceback frames, and
  structured cleanup lowering.
- Large module-initializer outlining without adding Python frames.
- Stable capability and source diagnostics, debug IR output, project-local
  `.rimera` intermediates, and forbidden-symbol checks.

The evidence is maintained in `spec/foundation.md` and the behavior tests in
`crates/rimera-compiler/tests/native_pipeline.rs`, runtime unit tests, and MIR
verifier tests.

## Known transitional limits

These are defects or deliberate boundaries, not completed architecture:

- User-facing dictionaries, sets, and frozensets use the permanent
  insertion-ordered managed hash table with generic `__hash__`/equality
  dispatch. Lookup, update, membership, and removal revalidate candidates after
  user callbacks mutate the table; equal dictionary-key replacement preserves
  the original stored key and insertion position. Mixed bool/int/float/complex
  keys obey the proven Python numeric equality/hash invariant.
- Native dictionary `|` and `|=` preserve insertion order and reuse stored
  hashes when merging another native dictionary; `|=` also accepts the normal
  mapping and iterable-of-pairs update sources. Broad CPython set layout and
  iteration-order equivalence remains a later corpus concern.
- Gate 4 recursive assignment/unpacking supports nested tuple/list targets,
  one star in any legal position per sequence level, chained targets, compiled
  class-body targets, and generic single-pass iterators. Extended unpacking
  preserves CPython's observable second `iter()` callback on the current
  iterator before draining the starred remainder, without source re-evaluation
  or rewind.
- Gate 3's owned builtin namespace, managed conversions, collection protocols,
  normalized builtin slicing, and forward/reverse iterator helpers are complete
  within their documented boundaries. Gate 6 synchronous generator lifecycle is
  now complete through the permanent native resume ABI. Remaining gaps are owned
  explicitly by later work: Gate 7 mutation/composition/final-audit reflection,
  async generators/iteration, and the final Unicode/codec corpus for
  surrogate-preserving Python string behavior.
- Class inheritance accepts Rimera user classes, `object`, and the supported
  list/tuple/dict/set/float/complex/bytes/bytearray/frozenset storage layouts.
  Those subclasses share builtin construction semantics, keep native payloads
  rooted across allocation/callbacks, and preserve mutable-base
  `super().__init__` behavior. Runtime `__bases__` mutation is
  transactionally validated across descendants. Native `__mro_entries__`, metaclass
  descriptor precedence, `property`, `staticmethod`, `classmethod`, custom
  descriptors, supported `__slots__` layouts, `__class__` cells, and
  zero-argument `super()` are available.
- Native class construction invokes supported descriptor `__set_name__` hooks,
  direct-base `__init_subclass__` hooks, and bottom-up class decorators through
  generic calls. Source and dynamic class creation now implement most-derived
  metaclass selection, managed prepared namespaces, custom `__new__` and
  `__init__`, `__mro_entries__`, metaclass data descriptors, and custom
  attribute hooks. Source class-body expressions, conditionals, `while`/`for` loops, local
  reads, name-target augmented assignment, item/attribute mutation, deletion, `raise`, ordinary
  `try`/`except`/`else`/`finally` (including loop-exit cleanup), nested classes, and methods in
  existing class control-flow blocks execute through compiled native class-body
  functions. Unsupported builtin storage layouts remain capability boundaries.
- Generic protocol dispatch now covers unary, binary/reflected, in-place, and
  comparison operations with `NotImplemented` fallback plus truth, length,
  containment, item access, calling, user iteration, and managed hashing.
  Remaining source/operator syntax is owned by later gates rather than an
  alternate runtime dispatch path.
- Gate 3 builtin failures now have a managed typed-exception path through the
  runtime context instead of relying on FFI message classification for the
  proven cases. Native differentials cover the required `ValueError`,
  `KeyError`, `BufferError`, `OverflowError`, `IndexError`, `TypeError`, and
  `ZeroDivisionError` cases and preserve exceptions raised by callbacks.
  Native complex arithmetic now keeps zero-imaginary values on the complex
  path, supports mixed real/complex arithmetic and two-argument power, rejects
  floor/modulo operations with Python exceptions, and matches CPython for the
  covered equality/hash, signed-zero, NaN, infinity, and zero-division cases.
- Gate 3 representation and formatting now use the native generic call path
  for user `__repr__`, `__str__`, and `__format__` methods while enforcing
  string return types. Float and complex repr match the covered CPython 3.12
  shortest-roundtrip display rules, Unicode repr printability uses Unicode
  15.0 category data, and recursive dictionary views render finite cycle
  markers. The supported int/float/complex/string format mini-language covers
  alignment, sign, alternate form, width/precision, zero-padding, grouping,
  `z`, scientific/general switching, special float values, and typed invalid
  spec failures. Locale-specific `n` transformations beyond the native core
  remain an explicit stdlib/platform-dependent boundary. The combined surface
  is proven by `gate3_repr_ascii_and_format_semantics_match_cpython_312`.
- Native `round()` now rounds the exact binary64 rational value through
  arbitrary-precision decimal ties-to-even arithmetic, returns arbitrary-size
  integers when `ndigits` is omitted, preserves CPython's signed-zero and
  subnormal behavior, and raises typed non-finite/overflow failures. Native
  float and mixed numeric `divmod()` derive quotient/remainder together with
  CPython's sign correction and signed-zero rules while retaining exact integer
  pairs and generic custom dunder/reflected dispatch. The combined behavior is
  proven by `gate3_round_and_float_divmod_semantics_match_cpython_312`.
- Gate 3 reverse iteration now covers bytes/bytearray, dictionaries, all three
  live dictionary views, ordinary builtin sequences, arbitrary-size ranges,
  custom `__reversed__`, and the legacy `__len__`/`__getitem__` fallback with
  callback exceptions preserved. Dict/view reverse iterators enforce the same
  size-change version rule as forward iteration. Range `len` now has the
  platform `Py_ssize_t` overflow boundary without weakening huge-range truth,
  indexing, slicing, or arithmetic iteration; `.start/.stop/.step`, `.count`,
  and `.index` use the managed object model, and canonical range equality/hash
  are preserved. Public behavior is proven by
  `gate3_reversed_and_range_semantics_match_cpython_312`, with the existing
  native slice-object path additionally covered by
  `range_slicing_uses_arbitrary_precision_native_bounds`.
- Managed slices now expose immutable `start`/`stop`/`step` plus arbitrary-
  precision `indices(length)` through the same normalization path used by
  native slicing, including explicit-`None`, `__index__`, negative-length, and
  zero-step behavior. Native-exporter memoryviews now share one native format
  model for optional `@` prefixes, `n/N/P`, platform-width `l/L`, scalar
  get/set/tolist, logical cross-format equality, ultimate-exporter hashability,
  and raw/non-contiguous byte conversion. This surface is proven by
  `gate3_slice_and_memoryview_remaining_semantics_match_cpython_312`. Gate 7
  Slice 8 now extends that exact memoryview core to Python-level PEP 688
  `__buffer__`/`__release_buffer__` providers using shared traced export leases
  rather than a second buffer representation; Gate 3 itself still claims builtin
  exporters (`bytes`, `bytearray`, and `memoryview`) only.
- Gate 3 dictionary views now complete the supported normal `dict_keys`,
  `dict_values`, and `dict_items` surface: views remain live/traced across
  dictionary mutation, reverse iterators enforce size-change invalidation,
  recursive repr is finite, `.mapping` exposes a live read-only mapping proxy,
  keys/items retain generic set-like operations/comparisons, and values views
  preserve identity-style equality. Public differential proof is
  `gate3_dictionary_view_finishing_matches_cpython_312`.
- Gate 3 managed-size accounting now charges retained list capacity and the
  permanent ordered hash table's entry/order/bucket/index allocations, while
  bytes/bytearray, memoryview metadata, slices, views, and legacy string-key
  dictionaries charge only storage they own. In-place growth refreshes the
  context's live-byte total before returning to generated code. The corrected
  accounting preserves both public low-heap outcomes—dead values are collected
  before rejection and genuinely reachable over-limit storage raises
  `MemoryError`—while the permanent startup kernel stays within the 8 KiB
  contract through lazy exception-type materialization.
- Gate 3 string compatibility is explicitly the Unicode-scalar domain, not the
  full Python code-point domain. Supported `chr`/`ord` scalar edges through
  `U+10FFFF` are differentially proven, while lone surrogates `U+D800..U+DFFF`
  deterministically raise a managed `ValueError`. A surrogate-preserving string
  representation plus surrogate-sensitive codec/error-handler behavior is
  assigned to the final Unicode/string-codec compatibility corpus after the
  numbered synchronous core gates; Gate 3 makes no broader string-domain claim.
- The Gate 3 synchronous builtin/type namespace now audits all 57 owned names
  through ordinary lookup and generic calls. Their Python names publish lazily,
  aliases remain callable after every audited name is rebound, and internal
  type metadata does not leak extra builtin names. `Ellipsis` is a rooted lazy
  singleton with internal `ellipsis` type identity, `__debug__` lazily resolves
  to `True`, and `NotImplemented` remains rooted. Reflection/dynamic-execution/
  import names remain later-gate exclusions. The 8 KiB startup proof remains
  green. Final Gate 3 release reachability declares only MIR-used runtime ABI
  imports; a semantically proven builtin `print("literal")` uses the runtime's
  literal-write ABI while rebound, aliased, keyword, and nonliteral calls retain
  ordinary Python call/print semantics. Full release symbol stripping produces
  a 485,064-byte `hello`, satisfying the 512 KiB Gate 3 contract.
- Gate 3 float/complex values now expose the small normal Python-visible value
  surface through managed attribute lookup and bound builtin calls: complex
  `real`/`imag`/`conjugate`, and float `real`/`imag`/`conjugate`, `is_integer`,
  exact `as_integer_ratio`, and CPython-shaped `hex`. Range, slice, and
  dictionary-view attributes remain owned by their earlier completed slices.
  Class-level `float.fromhex` remains outside the Gate 3 claim but is now
  implemented by Gate 7 Slice 7 through ordinary type attribute lookup and a
  bound managed callable. Public Gate 3 proof is
  `gate3_small_builtin_type_surfaces_match_cpython_312` plus a focused
  runtime managed-attribute regression. Gate 3 final acceptance retains this
  surface while meeting the release-size and low-heap contracts.
- Gate 3's minimal regression audit now has explicit public coverage for the
  previously uncovered set intersection/difference/symmetric-difference update
  methods, middle empty-slice insertion on lists/bytearrays, and negative-step
  slice deletion. The remaining Slice 22 risk list maps to the existing compact
  numeric/hash, constructors, repr/format, round, reversed/range, structured-
  exception/complex, and memoryview differentials instead of duplicating them.
- Source name/item/attribute augmented assignment and deletion use the proven
  native binding, item, attribute, and in-place protocol operations. `__iop__`
  dispatch falls back through normal/reflected operators as established by
  Gate 2. Gate 4's included synchronous source combinations are now implemented
  through this same path; exhaustive operator edge parity remains conformance
  audit work rather than a missing execution backend.
- Gate 6 completes synchronous generator allocation/resumption on the documented
  native ABI with exact GC ownership: source `yield`, suspension-aware MIR/codegen,
  `StopIteration.value`, `send`, `throw`, `close`, PEP 479, cleanup suspension,
  and generic `yield from` delegation are all public native behavior. Async
  generators and async iteration remain separate later-gate boundaries.

## Active dependency order

Gates 1–4 and Gate 6 are closed. The resume board's sole active compatibility
slice is **Gate 5 Slice 6 — propagation, chaining, groups, and cleanup completion**:

1. Gate 5 Slice 6 — continue the existing function/LEGB/structured-exception
   closure on the one binder/cell/exception/traceback/cleanup model. Slices 1–5
   are complete; then execute Gate 5 Slices 7–8 in numeric order from
   [`spec/gates/5/README.md`](gates/5/README.md).
2. Gate 7 Slices 1–8 are closed as deliberately pulled-forward ownership,
   namespace, identity, function-metadata, traceback/frame, generator-metadata,
   type-metadata, and Python-level buffer-provider prerequisites without
   promoting the Gate 7 compatibility row. Gate 7 Slices 9–10 and Gate 8
   synchronous context managers remain queued in
   [`spec/gates/7/README.md`](gates/7/README.md) and
   [`spec/gates/8/README.md`](gates/8/README.md); earlier gates do not imply
   their completion.
3. After Gates 5, 7, and 8 close, execute the queued post-core gates in numeric
   order: [Gate 9 imports/modules](gates/9/README.md),
   [Gate 10 async](gates/10/README.md),
   [Gate 11 dynamic compilation](gates/11/README.md), and
   [Gate 12 weak references/finalizers](gates/12/README.md).
4. Then run [Gate 13 language/runtime conformance](gates/13/README.md),
   [Gate 14 standard library](gates/14/README.md),
   [Gate 15 native extensions/platform ABI](gates/15/README.md),
   [Gate 16 packaging/ecosystem compatibility](gates/16/README.md), and
   [Gate 17 drop-in release qualification](gates/17/README.md). A general or
   drop-in compatibility claim remains forbidden until its matching gate
   closes with a published target and compatibility tier.

## Status-change rules

- Read the root `TODO.md` resume board and this ledger before selecting
  compatibility work. Keep exactly one active gate on the resume board.
- Select the earliest incomplete dependency needed by the requested feature;
  do not bypass HIR/MIR or create a compatibility backend.
- Follow every capability through source syntax, semantic meaning, MIR,
  verifier/liveness, Cranelift lowering, Rust ABI/runtime, GC behavior, public
  native execution, and differential failure tests.
- Do not promote a category based on scaffolding, ignored tests, hand-built MIR
  alone, or a fixture designed merely to avoid unsupported cases.
- Record newly proven behavior and remaining boundaries in this file during
  the same change, and mirror the gate checkbox in `TODO.md`. Keep
  `spec/foundation.md` for evidence, not aspirations.
- If implementation reveals that an earlier claim is too broad, narrow it
  immediately and add regression evidence.

## Anti-churn rules

- Extend the current native execution model; do not revive generated-C or
  introduce another value, exception, collection, or object representation.
- Do not implement framework-specific behavior before the underlying Python
  protocol exists.
- Do not multiply compiler crates or add generic `utils` modules to simulate
  architecture progress.
- Prefer one complete vertical capability slice over several disconnected
  runtime helpers.
- Preserve existing green behavior while expanding capability; a regression
  is work to fix, not a reason to weaken old tests.
