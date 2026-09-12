# Gate 9 Slice 10 — Import Hooks, Reload, Invalidation, and Reentrant Loading

## Goal

Implement the supported Python-visible extension points over the sole resolver
and module state machine.

## Integrated work

- Route import statements through the current managed `builtins.__import__`
  binding so supported rebinding and callbacks observe ordinary call semantics.
- Implement loader/spec callbacks needed by the static manifest and a native
  `importlib.reload` surface that reuses the existing module identity.
- Define cache invalidation for source/manifest changes and reject dynamic
  discovery beyond the compiled graph with stable capability diagnostics.
- Handle recursive hook calls, reload failure, exception chaining, and cache
  mutation without deadlock or duplicate initialization.

## Completion proof

Public differentials cover builtin hook rebinding, recursive imports, reload
identity/state, reload failures, invalidation, mutation, tracebacks, and
constrained heaps. Unsupported dynamic finders/loaders emit no artifact.

## Closure evidence

`import_hooks_reload_and_reentrant_loading_match_cpython` proves hook rebinding,
recursive import callbacks, reload identity/state/failure behavior, cache
invalidation, and authoritative-cache mutation through the single managed module
state machine. The final 209-test native pipeline audit retains this proof.

## DONE BY CHATGPT
