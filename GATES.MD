# Rimera Lite Zero-Partial Synchronous Core Contract

## Summary

The current epic is not complete. Gates 1–4 are complete. Gate 6 native synchronous generators are the sole active implementation gate selected by the Gate 4 closure; executable generator expressions already use the permanent native generator ABI. Function/exception closure, source generator lifecycle, reflection, and context managers still require their own end-to-end promotions.

Continue from the current worktree. Preserve every passing implementation and finish all started synchronous language areas plus synchronous context managers.

Successful completion means these areas no longer use `Partial`, `Substantial partial`, `Foundation only`, or `Active gate`:

- Exceptions and errors
- Function calling
- LEGB, scopes, and closures
- Core object model
- Core builtin types
- Operators and dunder dispatch
- Iterators
- Generators
- Classes, inheritance, MRO, descriptors, and metaclasses
- Builtins
- Comprehensions and unpacking
- Reflection and introspection
- Synchronous Python 3.12 syntax and edge behavior
- Synchronous context managers

Finished categories become `Implemented — conformance audit pending` unless exhaustive CPython parity is proven.

Imports/modules, async, dynamic compilation, weakrefs/finalizers, stdlib, platform bindings, and PyPI remain explicit later gates.

## Continuous Execution Rules

- Inspect the existing diff and `TODO.md` before editing.
- Begin at the sole active gate and execute gates in the order below.
- Never restart, revert correct work, or create parallel object/runtime/compiler models.
- A discovered prerequisite becomes immediate work; it is not a reason to stop.
- Do not report between gates or ask for another prompt.
- Keep the worktree compiling at gate boundaries.
- Use focused tests while developing. Run the consolidated workspace suite once per completed gate.
- Do not mark work complete from runtime scaffolding, hand-built MIR, dead exports, ignored tests, or capability diagnostics.
- Every completed feature must reach:

```text
source
→ Rimera syntax/HIR
→ verified MIR
→ Cranelift
→ documented Rust ABI/runtime
→ precise GC
→ public native fixture
```

Permanent restrictions remain: no generated C, CPython embedding, Python bytecode, MIR interpreter, RustPython runtime, or framework-specific paths.

## Gate 1 — Complete Classes, Metaclasses, and Descriptors

Replace the transitional direct class-member lowering with native class-body execution.

- Represent a class body as a compiled namespace-execution function receiving the context and prepared namespace.
- Support all existing synchronous statements inside class bodies: assignment, deletion, expressions, functions, nested classes, branches, loops, iteration, try/except/else/finally, raises, and supported unpacking.
- Resolve class locals through the prepared namespace, then globals and builtins. Methods must not close over ordinary class locals.
- Preserve exact construction order:
  1. evaluate bases and keywords;
  2. apply `__mro_entries__`;
  3. select the compatible metaclass;
  4. invoke `__prepare__`;
  5. execute the native body;
  6. invoke metaclass `__new__` and `__init__`;
  7. propagate `__classcell__`;
  8. invoke `__set_name__`;
  9. invoke `__init_subclass__`;
  10. apply decorators bottom-up.
- Complete class and metaclass data/non-data descriptor precedence.
- Complete `__getattribute__`, `__getattr__`, `__setattr__`, and `__delattr__` behavior without compiler special cases.
- Implement guarded subclasses for supported builtin storage types using one managed instance representation carrying user type identity and the native payload.
- Implement validated runtime `__bases__` reassignment with cycle, layout, metaclass, and C3 checks. Failed mutation leaves the original hierarchy intact.
- Invalidate descendant MRO and attribute caches after class namespace or base changes.
- Ensure failed class construction publishes nothing and releases temporary roots.

Acceptance includes arbitrary class control flow, custom prepared mappings, metaclass conflicts, hook ordering, descriptor precedence, builtin-container subclasses, `__bases__` mutation, forced GC, and failure cleanup.

## Gate 2 — Universal Dunder and Object Protocol Dispatch

Move protocol identity into shared `repr(u8)` enums in `rimera-abi`. HIR, MIR, Cranelift, and runtime must use those definitions instead of duplicated operator tables.

Implement:

- Unary protocols: positive, negative, invert, truth, index conversion.
- Binary, reflected, and in-place protocols: arithmetic, true/floor division, modulo, power, matrix multiplication, shifts, and bitwise operators.
- Comparison protocols, reflected subclass priority, identity syntax, and `NotImplemented` fallback.
- Hash, length, containment, item get/set/delete, call, iteration, reverse iteration, conversion, and formatting protocols.
- `__iop__` first, followed by normal/reflected fallback.
- Correct right-subclass reflected priority.
- Callable instances through `__call__`.
- Equality-without-hash becoming unhashable.
- Exact-type builtin fast paths that fall back to the same generic runtime protocol.

Add source and MIR support for the missing operator families, attribute/item augmented assignment, item deletion, and applicable special-method calls. Every fallible dispatch has an exception successor and a verified safepoint root plan.

## Gate 3 — Complete Builtin Values, Collections, and Builtins

Add permanent managed implementations for:

- `float`
- `complex`
- `bytes`
- `bytearray`
- `frozenset`
- `slice`
- dictionary view objects
- `memoryview`

For every builtin family implement exact type identity, construction, tracing, managed-size accounting, truth, equality, hash policy, `repr`/`str`, conversion, and applicable iteration.

Also:

- Replace restricted dictionary/set keys with generic `__hash__` and equality.
- Preserve insertion order and collision correctness.
- Make dictionary views live and traced.
- Complete normalized slicing for strings, bytes, bytearrays, lists, tuples, ranges, and memory views.
- Implement zero-step and invalid-index failures through structured exceptions.
- Complete mutation rules and immutable-type failures.
- Keep builtin types and builtin functions lazy to preserve heap-limit and release-size behavior.

Publish the complete synchronous builtin namespace supported by the finished core, including constructors, numeric helpers, collection helpers, iteration helpers, predicates, sorting/reduction helpers, formatting, and attribute helpers.

Exclude only builtins owned by later gates: `__import__`, `compile`, `eval`, `exec`, async helpers, and stdlib-dependent behavior.

## Gate 4 — Unpacking, Comprehensions, Expanded Calls, and Remaining Syntax

Implement the remaining synchronous syntax associated with started areas:

- Nested assignment targets.
- Starred targets in every legal position.
- `for` unpacking and dictionary unpacking.
- Call-site `*iterable` and `**mapping`.
- Duplicate keyword detection and left-to-right evaluation.
- List, set, and dictionary comprehensions with implicit scopes, filters, nesting, and closure capture.
- Generator-expression syntax, connected to Gate 6.
- Boolean operators with value-preserving short-circuiting.
- Chained comparisons with single evaluation.
- Slice expressions.
- Attribute and item augmented assignment.
- Assertions and deletion.
- Walrus expressions with correct scope behavior.
- F-strings and formatting conversion.
- Function and variable annotations.
- Pattern matching for Python 3.12 statement forms.

Use owned HIR and explicit MIR control flow. No runtime AST evaluation is allowed.

Acceptance must cover evaluation order, scope isolation, nested/starred failures, expanded binder errors, pattern binding cleanup, comprehension GC pressure, and stable diagnostics for genuinely later-gate syntax.

## Gate 5 — Close Functions, LEGB, and Structured Exceptions

The detailed implementation order is the eight-slice ledger in
[`spec/gates/5/README.md`](spec/gates/5/README.md). Gate 5 remains an
unpromoted closure audit while Gate 6 is active; only concrete Gate 5
prerequisites exposed by an active generator slice are repaired early.

Functions and scopes:

- Complete annotations, `__annotations__`, `__name__`, `__qualname__`, defaults, keyword defaults, decorators, and callable objects.
- Route expanded calls through the existing authoritative binder.
- Complete class, comprehension, annotation, global, nonlocal, deletion, unbound-local, and closure-cell behavior.
- Preserve independent activation records under recursion and mutual recursion.
- Complete function/code metadata needed by reflection.

Exceptions:

- Complete exception `args`, traceback, cause, context, suppression, and `with_traceback`.
- Normalize raised classes and instances.
- Support user exception subclasses.
- Complete bare re-raise, explicit cause, exception groups, `except*`, and handler-target cleanup.
- Preserve traceback positions and frame order through native calls.
- Preserve return, break, continue, and exception completion through all cleanup regions.
- Ensure active exception graphs survive GC and are released after handlers and frames finish.

After this gate, exceptions, calls, and LEGB become `Implemented — conformance audit pending`, with remaining behavioral mismatches recorded separately.

## Gate 6 — Complete Native Synchronous Generators

The detailed implementation order is the nine-slice ledger in
[`spec/gates/6/README.md`](spec/gates/6/README.md). Each slice is a substantial
vertical unit with synchronized compiler, runtime/ABI, and public-proof lanes;
only the integrated slice advances the ledger.

Extend the existing generator object and ABI instead of introducing a second implementation.

Compiler work:

- Add `yield`, `yield expression`, and `yield from` to syntax and HIR.
- Determine generator function kind without descending into nested scopes.
- Add suspension terminators, stable state IDs, normal and injected resume targets, and persistent-slot liveness.
- Verify suspension states, target validity, slot consistency, and generator-only usage.
- Emit a Cranelift resume dispatcher and compiled resume blocks. Never interpret MIR.

Runtime behavior:

- Generator calls bind arguments and allocate state without executing the body.
- Implement `next`, `send`, `throw`, `close`, self-iteration, repeated exhaustion, reentrancy checks, send-before-start errors, `GeneratorExit`, and PEP 479.
- Return generator values through `StopIteration.value`.
- Preserve locals, closure cells, handled exceptions, tracebacks, and pending cleanup across suspension.
- Implement complete `yield from` forwarding and delegated return extraction.
- Expose generator methods through ordinary descriptors and generic calls.
- Connect generator expressions from Gate 4.
- Clear persistent slots, delegates, and saved exception state on completion or close.

Acceptance includes lazy execution, multiple yields, delegation, thrown exceptions, `finally`, closure mutation, suspended GC survival, and post-completion collection.

## Gate 7 — Complete Reflection and Introspection

Publish reflection through managed builtin functions and ordinary protocol calls:

- `getattr`, `setattr`, `delattr`, `hasattr`
- `dir`, `vars`, `globals`, `locals`
- `type`, `isinstance`, `issubclass`
- `callable`, `repr`, `ascii`, `format`, `hash`, `id`
- Function signatures, annotations, defaults, closure metadata, and qualified names
- Exception and traceback inspection
- Code and frame metadata
- Generator ownership and suspension metadata
- Python-level PEP 688 `__buffer__` / `__release_buffer__` provider dispatch for user-defined objects, building on Gate 3's completed builtin exporter memoryview core

Expose stable Python-visible metadata without raw machine pointers.

Reflective class writes must update version tags and invalidate lookup caches. Frame/local views must have documented snapshot or live behavior matching CPython for the implemented operations.

After this gate, builtins and reflection become `Implemented — conformance audit pending`.

## Gate 8 — Synchronous Context Managers

Implement `with` as compiler-managed cleanup, not a runtime shortcut.

- Add syntax and HIR for multiple context items and supported assignment targets.
- Add special-method lookup that reads `__enter__` and `__exit__` from the type MRO and performs descriptor binding.
- Evaluate and enter managers left-to-right.
- Exit successfully entered managers right-to-left.
- Extend cleanup actions with `ContextExit`.
- Run exits during normal fallthrough, return, break, continue, assignment failure, and exception propagation.
- Pass `(None, None, None)` for normal exit.
- Pass the active exception type, value, and traceback for exceptional exit.
- Suppress propagation only when `__exit__` returns truthy.
- If `__exit__` raises, replace the pending completion and retain the previous exception as context.
- If a later manager fails during lookup or entry, clean up already-entered managers.
- Preserve context-manager cleanup across generator suspension, `throw`, and `close`.

Acceptance includes multiple-manager ordering, suppression, failed entry, replacement exceptions, all control-flow exits, generator suspension inside `with`, forced GC, and heap-limit behavior.

## Final Audit and Tracker Rules

Audit every Python 3.12 AST form associated with the included synchronous core. Each must either:

- execute through the full native pipeline; or
- belong to a named later architectural gate and produce a stable diagnostic with no artifact.

At final acceptance:

| Area | Required tracker status |
|---|---|
| Exceptions and errors | Implemented — conformance audit pending |
| Function calling | Implemented — conformance audit pending |
| LEGB/scopes/closures | Implemented — conformance audit pending |
| Core object model | Implemented — conformance audit pending |
| Core builtin types | Implemented — conformance audit pending |
| Operators/dunders | Implemented — conformance audit pending |
| Iterators | Implemented — conformance audit pending |
| Generators | Implemented — conformance audit pending |
| Classes/MRO/descriptors/metaclasses | Implemented — conformance audit pending |
| Builtins | Implemented — conformance audit pending |
| Comprehensions/unpacking | Implemented — conformance audit pending |
| Reflection/introspection | Implemented — conformance audit pending |
| Synchronous context managers | Implemented — conformance audit pending |

No included row may remain `Partial`, `Substantial partial`, `Foundation only`, or `Active gate`.

The sole subsequent active gate becomes imports, packages, modules, and `sys.modules`. Async, dynamic compilation, lifecycle features, stdlib, and ecosystem compatibility remain queued behind it.

## Verification Contract

For each gate add:

- Runtime unit tests for semantics, errors, tracing, and reclamation.
- MIR verifier and exact-root tests.
- Public native fixtures through the real build API.
- Representative CPython 3.12.11 stdout, stderr, and exit-status differentials.
- Negative tests proving unsupported later-gate syntax emits no artifact.
- Final artifact scans excluding CPython, generated-C, `setjmp`, and `longjmp`.

At each gate boundary run once:

```bash
cargo fmt --check
cargo clippy --workspace -- -D warnings
cargo test --workspace
cargo test --doc --workspace
git diff --check
```

Final acceptance additionally requires:

```bash
cargo build --release
```

Then verify:

- The public synchronous corpus compiles and runs.
- `/opt/homebrew/bin/python3.12` reports version 3.12.11 and matches representative fixtures.
- Release `hello` remains within 512 KiB.
- No ignored tests, placeholder panics, dead ABI exports, generic development diagnostics, or generated C exist.
- `TODO.md`, `AGENTS.md`, `spec/compatibility.md`, and `spec/abi-v1.md` agree.
- Exactly one later active gate is recorded.

The final report is emitted only after all eight gates pass. It must contain completed gates, public interfaces, exact commands and test totals, differential evidence, GC evidence, forbidden-symbol results, release size, tracker status, and explicitly deferred later architectures.
