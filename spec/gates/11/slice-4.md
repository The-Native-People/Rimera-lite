# Gate 11 Slice 4 — exec namespaces and declarations

The public acceptance fixture is `tests/fixtures/basic/gate11_exec_scope.py`.
It compares native stdout and failure behavior with CPython 3.12.11 under a
262,144-byte managed heap, followed by a forbidden-symbol artifact scan.

## Execution contract

`exec(source, globals=None, locals=None, *, closure=None)` accepts one to three
positional arguments. `closure` is its only accepted keyword. Namespace
selection, validation, and builtin insertion follow Slice 3. Successful exec
returns `None`, including when a supplied code object computes a return value.

Ordinary top-level stores/deletes operate on locals through the existing
namespace MIR/ABI operations. Custom mappings and dict subclasses receive
`__setitem__`/`__delitem__`; augmented assignment preserves read/write order.
Earlier successful writes remain visible if a later callback or statement
raises. Exec is not a namespace transaction. Lookup/write/delete callback
exceptions are propagated without replacement by a generic runtime error.

Explicit `global` declarations target the globals dictionary directly,
bypassing dict-subclass write/delete overrides. Missing direct deletions raise
`NameError`. Dynamic declarations do not modify the enclosing source's lexical
scope plan. Module-level `nonlocal` remains a syntax failure. A standalone
`pass` statement remains outside the accepted dynamic syntax subset.

Definitions bind into locals, but their functions resolve globals in the
supplied globals dictionary, not the separate locals mapping. Nested class
suites use their own prepared namespace plus those globals. Calling exec/eval
from an existing class suite uses its actual prepared namespace, including
custom metaclass mappings. The prerequisite `type.__new__`/super path delegates
to the existing class constructor; no alternate class model is introduced.

Every managed function captures its builtins namespace at creation. Replacing
`globals['__builtins__']` affects later compilations/definitions, not existing
functions; mutating the captured dictionary remains observable. Function
`__globals__` and `__builtins__` expose those read-only identities. Globals are
traced directly; custom builtins use Slice 5's weak-function/strong-value GC
association, so retained functions keep their namespace alive without pinning
dead functions.

For code with free variables, closure must be an exact tuple of managed cells
with exactly the code's free-variable count. Missing/wrong-length/non-cell
tuples raise `TypeError`. A non-None closure is rejected for source text and
for code without free variables. Execution uses the original cells, so writes
through `nonlocal` are observable to their owner. Ordinary function code uses
the normal binder and native activation path rather than a second executor.

## Evidence and boundary

`gate11_eval_and_exec_scope_conformance` covers separate namespace writes,
explicit global reads/writes/deletes, class/function definitions, live globals,
class-body and custom-prepared-mapping composition, partial failure state,
callback order, closure-cell mutation and rejection, dict-subclass operations,
captured builtins, and retained native functions under allocation pressure.
Runtime unit tests independently check validation order and namespace mutation
on failure. The earlier Slice 1–2 native fixture remains a regression test.

Fine-grained reclamation/cache policy is closed by Slice 5 and supported
cross-feature composition by Slice 6. Adversarial composition is not promoted
here. Type construction keyword expansion beyond the existing
constructor contract and a general Python 3.12 dynamic-code claim are not
implied by this slice.
