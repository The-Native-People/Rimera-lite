# Rimera native compiler specification

**Status:** authoritative architecture. The native foundation and first source
slice are implemented; later Python capability sections remain roadmap work.

## Product

Rimera compiles a Python project to a standalone native executable. Its normal
path is:

```text
Python + pyproject.toml + lockfile
 -> parser -> module/capability resolver -> semantic analysis
 -> Rimera HIR -> Rimera MIR -> Rimera LIR/Cranelift
 -> native object files -> link Rust runtime + selected libraries -> executable
```

Rimera never emits C as an intermediate representation. It does not embed
CPython, RustPython, or a bytecode interpreter. New Python semantics are IR,
Cranelift, and Rust-runtime work. C may exist only at a deliberate foreign ABI
boundary and is never the implementation route for new language features.

## Honest outcomes

Every build either compiles, is capability-denied before artifact output, or
is rejected as unsupported with a stable source diagnostic. It never silently
changes Python behavior or claims Python 3.12 compatibility without category
specific differential proof.

## Inputs and reproducibility

A build request contains the entry point, reachable source/resources,
pyproject metadata, lockfile, target, profile, runtime ABI version, and
granted capabilities. Resolution is implemented in Rust against lockfile data;
cache keys include all of these inputs. Builds do not depend on a venv, a
system Python installation, or unpinned network state.

Compiler intermediates are cached under `<project>/.rimera/objects/`; object
names are deterministic for the entry point, output path, target, and profile.
`RIMERA_CACHE_DIR` may relocate the `.rimera` directory without placing
intermediates beside user artifacts.

## Front end

The parser preserves Python 3.12 syntax and spans without runtime effects. The
module resolver builds independent namespaces, package initialization order,
relative imports, aliases, resources, and cycles. Capability analysis records
the reachable operation, module, location, and stable diagnostic code.

Semantic analysis owns names, scopes, imports, control-flow facts, class and
call-shape facts, effects, and specialization evidence. Lack of proof chooses
a generic native runtime operation, never a guessed specialization.

## Rimera IR

HIR represents Python semantics. MIR is SSA CFG with explicit blocks, values,
GC roots, exception edges, calls, guards, and source spans. LIR represents
target ABI, layout, relocations, and Cranelift operations. MIR is the only
compiler-owned executable representation; it is serializable and verified
after every transform.

Values are stable `RValue` values: immediates or opaque generational handles.
Rust owns heap allocation, tracing, collection, and destruction. A fallible
operation uses `operation(context, inputs..., output*) -> RStatus`; success
writes output, exception records state in the context, and Rust panics never
cross the ABI.

The collector is precise, non-moving, and stop-the-world. Generated code
publishes compact liveness-derived shadow roots at runtime safepoints; managed
objects trace child `RValue`s through an iterative worklist. Collection uses
adaptive managed-byte thresholds and supports an optional per-context budget.
Its lifecycle phase reserves weak-reference and finalizer processing without
claiming those Python behaviors before the object model implements them.

`try` lowers to normal, handler, and cleanup blocks. Exceptions, cause,
context, suppression, and immutable traceback frames live in `RimeraContext`.
There is no `setjmp`, `longjmp`, global exception buffer, or host unwinding.
Return, break, and continue execute compiler-planned cleanup actions in
inner-to-outer order; a transfer issued by `finally` replaces the pending
completion reason. `except*` uses recursive runtime subgroup splitting,
executes every matching handler, clears handler bindings, and merges deferred
handler failures with the unhandled remainder before propagation.

## Native emission

Static MIR lowers directly to Cranelift and emits one object per module.
Large module initializers are outlined at HIR-to-MIR boundaries into ordered
internal module chunks before Cranelift emission. Chunks share the module
globals and context, return through the normal `RStatus` ABI, and are invoked
by a small native module driver. They are compiler implementation units rather
than Python frames, so an exception escaping a chunk produces exactly one
`<module>` traceback frame at the originating source line.
Linking selects only reachable Rust runtime archives, native stdlib objects,
resources, and platform libraries. Release linking garbage-collects sections
and emits a manifest of target, ABI, objects, packages, hashes, and
capabilities.

Dynamic code, when granted, parses into the same HIR/MIR pipeline and uses a
native Cranelift compiler inside the owning context. Static builds omit it.

## Runtime and conformance

The Rust runtime is split into context, value/heap, exception/traceback,
object model, stdlib, package, and FFI components. It has no interpreter or
CPython dependency. Pinned CPython 3.12 references define behavior; every
supported feature requires public-entry native fixtures, differential tests,
negative capability tests, and no-legacy-symbol checks. Frameworks are
acceptance workloads, never compiler branches.

## Migration rule

The existing generated-C implementation is frozen reference material. It
receives no behavior. It may be deleted only after equivalent native proof;
the final native compiler contains no generated-C lowering path.

## Acceptance

A feature is complete only with focused tests, workspace tests, IR verifier,
ABI-contract checks, reproducible outputs, no-legacy-symbol proof, and the
release-size budget. Partial progress is recorded as evidence, not a
compatibility claim.
