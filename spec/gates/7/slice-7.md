# Gate 7 Slice 7 — Type Method Tables, Class Metadata, and Python 3.12 Type Parameters

## Goal

Finish reflection over builtin/user types and implement the Python-visible
metadata owned by Python 3.12 generic declarations.

## Integrated work

- Publish builtin and user type attributes through ordinary class/metaclass
  lookup, including deferred class-level methods such as `float.fromhex` rather
  than compiler intrinsics.
- Expose supported `__dict__`, `__bases__`, `__mro__`, `__name__`,
  `__qualname__`, `__module__`, slots, descriptors, and subclass-visible method
  tables with correct read/write/delete rules.
- Add owned syntax/HIR/sema/MIR/runtime support for Python 3.12 function/class/
  alias type parameters and their `__type_params__` metadata, scope, evaluation
  order, bounds/default boundaries, and stable diagnostics.
- Keep metaclass descriptor precedence and class mutation transactional; type
  metadata must not reveal internal runtime names or storage layouts.
- Trace type-parameter/bound/class graphs and preserve lazy builtin publication
  and release-size constraints.

## Implemented surface

- Unbounded Python 3.12 PEP 695 parameters `T`, `*Ts`, and `**P` are owned by
  syntax/HIR/sema/MIR/Cranelift and become traced managed `TypeVar`,
  `TypeVarTuple`, and `ParamSpec` objects. Generic functions/classes publish
  `__type_params__`; `type Alias[T] = T` publishes a managed `TypeAliasType`
  with stable `__name__`, `__type_params__`, and `__value__` identity.
- Lazy bound/constraint evaluation remains intentionally deferred. Supported
  syntax reaches the native path; bound/constraint forms fail before artifact
  output with the stable `RIM-CAP-G7-03` diagnostic instead of fabricating eager
  semantics.
- Class metadata now separates Python-visible `__qualname__` from the
  module-qualified class/instance representation, seeds `__module__` and nested
  `__qualname__` before class-body execution, exposes read-only `__mro__`, and
  preserves Python write/delete rules for the audited metadata.
- The deferred class-level `float.fromhex` method is a normal bound managed
  callable reached through type attribute lookup, including supported float
  subclasses; no compiler intrinsic or second method table was added.

## Completion proof

Public CPython 3.12.11 differentials
`gate7_type_parameters.py`, `gate7_type_metadata.py`, and
`gate7_type_parameters_gc.py` cover type-parameter kind/scope/identity,
function/class/alias metadata, nested class metadata, `float.fromhex`, mutation,
and forced GC. `gate7_cap_type_param_bound.py` proves the remaining lazy-bound
boundary is stable and emits no artifact. Focused Slice 7 native-pipeline proof
passes 4/4 tests.

## DONE BY CHATGPT
