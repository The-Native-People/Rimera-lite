# Gate 7 Slice 4 — Function, Callable, Closure, Signature, and Code Metadata

## Goal

Publish the metadata produced by Gate 5 without exposing backend addresses or
creating a second signature model.

## Integrated work

- Expose supported function `__name__`, `__qualname__`, `__annotations__`,
  `__defaults__`, `__kwdefaults__`, `__closure__`, `__code__`, and writable-field
  rules with stable identity.
- Represent closure cells and code metadata as managed Python-visible objects
  backed by the authoritative binder, scope analysis, source spans, and compiled
  function contract.
- Expose parameter ordering/kinds, local/free/cell names, filename, first line,
  and flags required for the supported synchronous core without publishing raw
  function pointers or Cranelift details.
- Cover functions, lambdas, methods, class bodies, comprehensions, decorated
  callables, aliases, and recursive closures through ordinary attributes.
- Trace metadata graphs and reclaim dead functions/cells/code objects while
  preserving independently retained metadata.

## Completion proof

Public differentials cover reads, permitted writes, validation failures,
identity, nested qualified names, signatures, cells, recursion, decorators,
forced GC, and heap-limit failures without using `inspect`. Integrated proof is
`gate7_slice4_function_closure_signature_and_code_metadata_match_cpython_312`
over `gate7_function_code_metadata.py`, plus
`gate7_slice4_retained_code_and_closure_metadata_survive_gc_pressure` over
`gate7_function_code_metadata_gc.py`. The runtime proof
`ffi::tests::gate7_slice4_code_metadata_allocation_is_low_heap_atomic` verifies
that failed metadata allocation raises managed `MemoryError`, leaves the ABI
output untouched, and publishes no partial function/code object.

## DONE BY CHATGPT
