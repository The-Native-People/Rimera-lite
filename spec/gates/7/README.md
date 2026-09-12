# Gate 7 — Complete Reflection and Introspection

Gate 7 publishes Python-visible metadata and reflective operations through the
existing managed object model, generic call/attribute protocols, native frames,
and runtime context. It must not expose raw machine pointers, create a shadow
object model, depend on `inspect` or another stdlib module, or introduce runtime
Python evaluation.

Gates 5 and 6 are complete, and Gate 7 is now complete. All ten slices extend
the established Gate 3/5/6 object, metadata, frame, generator, and lifecycle
contracts through one managed reflection model; Gate 8 synchronous context
managers are the next active compatibility gate.

Every slice must reach the complete native path:

```text
Python source -> owned syntax/HIR -> semantic analysis -> verified MIR
-> Cranelift -> documented Rust ABI/runtime -> precise GC
-> public CPython 3.12.11 differential
```

## Synchronized slice contract

- The slice size targets sustained GPT-5.6 Sol implementation work: each slice
  closes a meaningful reflection family without requiring another prompt
  between compiler, runtime, metadata, and proof work.
- Work in numeric order and keep exactly one Gate 7 slice active after the gate
  is promoted.
- A slice may use coordinated compiler, runtime/ABI, and proof lanes. Give each
  file one active owner and synchronize first on Python-visible metadata,
  snapshot/live semantics, mutation effects, and ABI ownership.
- The compiler lane owns syntax/HIR/sema/MIR/verifier/Cranelift reachability and
  source spans. The runtime lane owns managed metadata, attribute/builtin
  operations, cache invalidation, tracing, and reclamation. The proof lane owns
  public fixtures, CPython differentials, negative diagnostics, heap-limit/GC
  cases, artifact scans, and documentation evidence.
- A slice integrates only when ordinary compiled source reaches its reflective
  behavior through generic protocols and public proof. Runtime metadata that no
  source program can observe is scaffolding, not completion.
- Repair concrete prerequisites inside the active slice using their existing
  owner; do not duplicate functions, frames, tracebacks, generators, buffers,
  dictionaries, or type namespaces.
- Use focused checks during development. At a slice boundary run formatting,
  warning-denied clippy, relevant workspace tests, doc tests, and
  `git diff --check` once.
- Do not promote compatibility rows after individual slices. Slice 10 owns the
  single audited Gate 7 promotion.

## Progress ledger

- [x] Slice 1 — ownership, observability, and differential matrix
- [x] Slice 2 — namespace views: `globals`, `locals`, `vars`, and `dir`
- [x] Slice 3 — identity, type relations, and reflective attribute helpers
- [x] Slice 4 — function, callable, closure, signature, and code metadata
- [x] Slice 5 — exception, traceback, and frame inspection
- [x] Slice 6 — generator identity, state, and suspension metadata
- [x] Slice 7 — type method tables, class metadata, and Python 3.12 type parameters
- [x] Slice 8 — Python-level PEP 688 buffer providers
- [x] Slice 9 — mutation invalidation, composition, GC, and failure atomicity
- [x] Slice 10 — final audit, documentation, and gate closure

## Slice order

1. [Ownership, observability, and differential matrix](slice-1.md)
2. [Namespace views: `globals`, `locals`, `vars`, and `dir`](slice-2.md)
3. [Identity, type relations, and reflective attribute helpers](slice-3.md)
4. [Function, callable, closure, signature, and code metadata](slice-4.md)
5. [Exception, traceback, and frame inspection](slice-5.md)
6. [Generator identity, state, and suspension metadata](slice-6.md)
7. [Type method tables, class metadata, and Python 3.12 type parameters](slice-7.md)
8. [Python-level PEP 688 buffer providers](slice-8.md)
9. [Mutation invalidation, composition, GC, and failure atomicity](slice-9.md)
10. [Final audit, documentation, and gate closure](slice-10.md)

Gate 7 is closed. Reflection/introspection and its owned builtin surfaces are
`Implemented — conformance audit pending`; the documented import, dynamic-code,
weak-reference, async, stdlib, and conformance boundaries remain separate later
gates.

A concrete Slice 1/2 prerequisite was pulled forward without promoting the
broader import gate: simple `import name` / `import name as alias` now has one
native syntax/HIR/MIR/Cranelift/ABI/runtime path backed by cached managed module
objects. Only `inspect` and `weakref` module shells are currently registered so
namespace reflection can compose with real module values. Their stdlib APIs are
not implemented or claimed. Arbitrary modules, `from` imports, dotted/package
loading, module-source execution, public `sys.modules`, and `__import__` remain
later import/stdlib work. Dynamic compilation, weak-reference behavior, async
inspection, and broad stdlib metadata remain later-gate work.

Slices 1–10 are closed with source-to-native CPython differentials, MIR/sema
ownership proof, generic type/attribute reflection, managed
function/code/cell/traceback/frame/generator/type-parameter metadata, PEP 695
generic declaration ownership, Python-level PEP 688 provider leases, descendant
and metaclass-observer invalidation, constrained-heap mutation atomicity,
cross-family composition, retained-metadata/exporter-cycle GC proof, stable
no-artifact deferred boundaries, the preserved startup/release constraints, and
the full warning-denied/release audit. Gate 8 and Gate 9 Slice 1 have since closed; Gate 9 Slice 2
is now the sole active compatibility slice.
