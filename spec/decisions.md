# Architecture decisions

## Clean native architecture

Rimera is a clean-room native compiler. `Trash/` is read-only behavioral
evidence. New source flows through Rimera-owned syntax, HIR, MIR, LIR,
Cranelift object emission, and Rust-runtime linking. RustPython parses syntax
only; Rimera does not emit C, embed CPython, or include a bytecode interpreter.

Unsupported behavior receives a stable diagnostic before artifact output.

## Stable values and tracing collection

Every executable MIR value is a 16-byte ABI `RValue`. Immediate values remain
inline; managed values are opaque generational handles in a non-moving
mark-and-sweep heap owned by `RimeraContext`. Generated functions publish live
slots through explicit root frames at runtime safepoints.

Generated code never dereferences runtime objects. Arbitrary-size integers and
strings share the same value representation.

Collection is precise, non-moving, stop-the-world mark-and-sweep. Marking uses
an iterative worklist and every heap object exposes its child-value tracing and
managed-size contracts. Collection proceeds through root discovery, marking,
lifecycle processing, and sweeping; the lifecycle boundary reserves the proper
location for future weak references and finalizers without claiming those
Python behaviors today.

Generated functions publish compact liveness-derived shadow roots at every
runtime safepoint. Runtime operations separately protect unpublished temporary
values, and contexts provide roots for future modules and exception state.
Collection pressure is measured in managed bytes with an adaptive threshold;
an optional context limit provides deterministic bounded-heap failure.

Generational, incremental, concurrent, compacting, weak-reference, and
finalizer behavior remains deferred until its corresponding runtime and Python
semantics exist. Those features must extend these tracing and lifecycle
boundaries rather than introduce a second collector.

## Native emission and linking

Cranelift emits target object files directly. On macOS arm64, `clang` is only
the system linker driver combining objects with the Rust runtime static archive
and platform libraries. No build step accepts or generates C source.
