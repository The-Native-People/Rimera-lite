# Gate 7 Slice 1 — Ownership, Observability, and Differential Matrix

## Goal

Define the exact Python-visible reflection surface and its single owners before
adding metadata or operations.

## Integrated work

- Inventory every Gate 7 builtin, special attribute, namespace view, function/
  code/frame/traceback/generator field, type-parameter form, and PEP 688 hook.
- Map each observable to its syntax, HIR, MIR, ABI, runtime object, lifetime,
  mutation, and public-test owner; remove or consolidate duplicate contracts.
- Specify which namespace/frame views are live and which are snapshots, their
  ordering, mutability, identity, and behavior after the originating activation
  completes.
- Build a CPython 3.12.11 matrix for values, types, errors, side effects,
  mutation invalidation, repr, identity stability, GC, and low-heap failure.
- Assign every gap to Slices 2–9. Dynamic compilation, async metadata, broad
  stdlib reflection, and unregistered/general import forms retain stable
  diagnostics and emit no artifact. A narrow managed import/module prerequisite
  for `inspect` and `weakref` shells is pulled forward here because Slice 2 must
  observe real module namespaces without depending on a future shadow loader.

## Single-owner matrix

| Observable family | Syntax/HIR + sema owner | MIR/Cranelift owner | ABI/runtime owner | Lifetime / mutation rule | Gate 7 owner / proof |
| --- | --- | --- | --- | --- | --- |
| `globals()`, module `locals()`, zero-arg `vars()` | ordinary global builtin lookup/call; no intrinsic syntax node | generic `Call`; module globals remain context-owned | context globals dictionary + generic builtin call | one live module dictionary; writes are immediately visible to compiled global lookup | Slice 2; `gate7_namespace_views.py` |
| pulled-forward simple `import name` / `import name as alias` | owned import syntax/HIR and normal binding resolution; sema admits only the explicit native registry | one `ImportName` MIR operation lowered directly by Cranelift | `rimera_import_name` returns a cached traced `ModuleObject` with a live managed namespace | repeated imports preserve identity; publication is failure-atomic; current shells are `inspect` and `weakref` only | Slice 1 prerequisite; `gate7_import_foundation.py` + runtime cache/GC proof |
| function `locals()` / zero-arg `vars()` | scope plan supplies ordered parameters then first bindings | `ReflectionScopeConfigure` + `ReflectionLocalRegister` declare authoritative compiled cells | active-call locals snapshot refreshed from those cells | same managed dict identity within one activation; user-only entries survive refresh; compiled bindings overwrite their own keys; retained snapshot survives return | Slice 2; public differential + GC fixture |
| class-body `locals()` / zero-arg `vars()` | class HIR owns prepared namespace | class-body MIR configures its existing namespace; no duplicate frame namespace | prepared class namespace / mapping protocol | live write-through mapping during class execution; class publication consumes this same namespace | Slice 2 |
| list/set/dict comprehension `locals()` | comprehension scope plan owns iteration locals/free bindings | hidden native comprehension activation registers real cells and marks inlined-comprehension observation | active-call overlay over nearest non-comprehension owner | CPython 3.12 inlined view: owner mapping plus transient comprehension locals; transient names are restored/removed when hidden activation ends | Slice 2 |
| generator-expression `locals()` | generator-comprehension scope owns `.0`, loop locals, referenced frees | ordinary generator hidden function; reflection scope is frame-like, not inlined | suspended generator persists registered cells and one refreshed locals snapshot | `.0`, loop locals, and referenced frees live in the suspended frame snapshot across resumes | Slice 2, generator identity/state expansion remains Slice 6 |
| source-generator `locals()` | Gate 6 function scope metadata | existing generator resume MIR plus reflection local registration | generator object persists local cells/snapshot across suspension | same snapshot identity across yields; refresh observes current compiled cells; generator completion drops its internal snapshot root while independently retained dicts remain managed | Slice 2; generator metadata fields remain Slice 6 |
| one-arg `vars(obj)` / `__dict__` | generic builtin and attribute resolution | generic `Call`/attribute path | ordinary `__dict__` attribute protocol | returns the object's authoritative live dictionary/mappingproxy where supported; missing `__dict__` raises `TypeError` | Slice 2 |
| `dir()` / `dir(obj)` / `__dir__` | generic builtin lookup/call | generic call path only | current locals/default object-name synthesis or ordinary bound `__dir__` callback; result sorted by list semantics | zero-arg uses current locals; default object form observes instance/class/base/metaclass names; custom callback side effects/errors are preserved | Slice 2; mutation invalidation stress in Slice 9 |
| `type`, `isinstance`, `issubclass`, `getattr`, `setattr`, `delattr`, `hasattr`, `callable`, `repr`, `ascii`, `format`, `hash`, `id` | existing expression/call/attribute HIR | generic call/attribute/protocol MIR | managed type/attribute/hash/format protocols | no compiler intrinsic aliases; callback exception/identity rules remain authoritative | Slice 3 |
| function `__name__`, `__qualname__`, `__annotations__`, `__defaults__`, `__kwdefaults__`, `__closure__`, `__code__`; code/signature/cell fields | Gate 5 function/scope metadata is authoritative | existing `MakeFunction` + generic attributes | managed function/cell/code metadata; never raw code pointers | writable fields follow Python validation; metadata graphs are traced and independently retainable | Slice 4 |
| exception `args`, cause/context/traceback; traceback and frame metadata | Gate 5 exception ownership + compiler source spans | compiled exception edges/native traceback publication | managed exception/traceback/frame metadata | retained tracebacks keep required frame/local graphs alive; cycles collect when released | Slice 5 |
| generator `gi_*` identity/state/frame/code/delegate metadata | Gate 6 generator contract | existing state-machine/resume path only | managed generator/frame metadata | never-started/running/suspended/delegating/terminal transitions stay coherent and resume addresses remain private | Slice 6 |
| type `__dict__`, bases/MRO/name/qualname/module/slots/method tables; Python 3.12 type parameters | class/type-parameter syntax/HIR/sema | class/type metadata lowering | managed type/metaclass/type-parameter objects | ordinary descriptor/metaclass mutation rules; no runtime-layout exposure | Slice 7 |
| PEP 688 `__buffer__` / `__release_buffer__` | ordinary special-method lookup | generic call/buffer bridge | Gate 3 buffer core + Python-provider lifecycle | provider/view graphs traced; release exactly once | Slice 8 |
| cache invalidation, reentrant reflection, cross-family GC/`MemoryError`, transactional mutations | all existing owners, no new shadow model | existing generic operations only | context/object cache and precise-GC owners | failed reflective changes are atomic; stale class/descriptor/dir observations forbidden | Slice 9 |

## Deferred / excluded boundaries

- The pulled-forward import foundation is intentionally narrow. `inspect` and
  `weakref` are importable **module shells** so module identity and namespace
  reflection have a real managed owner; their stdlib APIs are not implemented.
  Unregistered modules, `from ... import ...`, dotted/package imports, Python
  module-source execution, public `sys.modules`, and direct `__import__` remain
  unsupported. Unregistered simple imports fail before artifact output.
- Stable global references to `eval`, `exec`, or `compile` are rejected by sema
  as `RIM-CAP-G7-02`, including aliasing such as `runner = eval`; a locally or
  globally rebound user value with the same name remains an ordinary binding.
- Async function syntax remains `RIM-CAP-001` and emits no artifact.
- At Slice 1 closure, Python 3.12 type-parameter syntax was intentionally left
  to Slice 7. Slice 7 has since implemented unbounded `T`/`*Ts`/`**P`
  function/class/type-alias parameters; lazy bound/constraint forms remain the
  stable `RIM-CAP-G7-03` no-artifact boundary.
- Broad stdlib reflection is not implemented by substituting runtime Python,
  `inspect`, bytecode evaluation, or CPython embedding.

## Differential / ownership proof

- `/opt/homebrew/bin/python3.12` is the public oracle (Python 3.12.11 for this
  slice cycle).
- `gate7_namespace_views.py` pins values, types, identity, ordering, mutations,
  custom `__dir__`, class/module/function/comprehension/genexpr/generator scope
  behavior, and ordinary native-only artifact output.
- `gate7_namespace_views_gc.py` pins retained snapshot/generator graphs and dead
  activation cycles under a 96,000-byte managed heap limit.
- `gate7_import_foundation.py` proves cached module identity, aliases, module
  `__name__`/`__dict__`, namespace mutation, and function/class-scope import
  binding against CPython 3.12.11 without claiming any `inspect`/`weakref` API.
- `gate7_slice1_deferred_reflection_boundaries_are_stable_and_emit_no_artifact`
  covers dynamic compilation, unregistered/dotted/`from` import forms, async
  syntax, and deferred type parameters with stable spans/no artifact.
- `gate7_local_observation_order_preserves_parameters_then_first_bindings` and
  `gate7_namespace_reflection_mir_declares_authoritative_scope_ownership` pin
  sema ordering plus the single MIR/ABI ownership path.
- Runtime regressions `gate7_import_heap_failure_is_memory_error_and_publication_is_atomic`
  and `gate7_reflection_call_heap_failure_is_memory_error_and_source_namespace_survives`
  pin common `MemoryError` classification, failure-atomic module publication,
  and source-namespace survival under a zero-slack managed heap.

## Completion proof

Focused Gate 7 proof is green: 2 compiler ownership tests, 5 public native
pipeline tests, and 3 runtime tests. The shared Slice 1–2 boundary is also green:
`cargo fmt --check`, warning-denied workspace clippy, `cargo test --workspace`
(279 passed, 0 failed/ignored, including doc tests), and `git diff --check`.
Every Gate 7 observable has one implementation owner, one lifetime rule, and one
slice; later observables remain assigned to Slices 3–9.

## DONE BY CHATGPT
