# Gate 11 Slice 3 — eval namespaces and closure reads

The public acceptance fixture is `tests/fixtures/basic/gate11_eval_scope.py`.
It compares stdout and exception types/messages against CPython 3.12.11 and
runs as a native executable with a 262,144-byte managed heap. Artifact symbols
are checked for forbidden interpreter/CPython/setjmp/longjmp paths.

## Execution contract

`eval(source, globals=None, locals=None)` accepts one to three positional
arguments and no keywords. Source is an executable managed code object or one
of the source forms documented in Slice 2. Leading spaces/tabs are removed
from source text before expression parsing. A supplied code object retains
its original mode: expression code returns its value, statement code returns
`None`, and interactive code keeps its display behavior.

An omitted/`None` globals argument selects the caller's globals. Omitted/`None`
locals selects explicitly supplied globals, otherwise the caller's current
locals. Globals must be a dict, including a supported native dict subclass;
locals may be an item-access mapping. A supplied empty dict is not treated as
omitted. Invalid namespaces fail before source compilation or execution.

After namespace validation, an absent `globals['__builtins__']` is inserted
from the caller's execution builtins, before source/type/syntax validation.
This insertion survives a subsequent syntax/type failure, matching CPython.
An existing value, including an empty dict or `None`, is not overwritten.

Top-level name reads consult locals, then raw globals, then execution builtins.
Custom locals and dict-subclass locals receive ordinary item callbacks.
`KeyError`, including a subclass, means lookup may continue; any other
exception retains its identity and traceback. A final miss is `NameError`.
Non-mapping builtins retain their item-access failure rather than silently
falling back to the kernel. Builtin names such as `print` are not statically
substituted in capability-enabled code.

Implicit locals expose the existing activation snapshot, including closure
cells already captured by that activation. Merely naming an enclosing variable
inside a string does not cause a new lexical capture. Code objects containing
free variables are rejected by `eval`; a locals dict is not a closure tuple.
Executable ordinary function code uses the existing binder and native call
path, including empty variadic binding and lazy generator creation. Required
arguments are diagnosed by that same binder. Synthetic reflection code without
a native entry is rejected instead of dereferencing an invalid address.

## Evidence and boundary

`gate11_eval_and_exec_scope_conformance` covers namespace precedence/identity,
implicit locals and captured-cell reads, mapping callback order and failures,
dict subclasses, failed-source builtin insertion, restricted builtins,
argument errors, existing function/generator code, and optimized expression
results. The prerequisite optimization repair preserves lexical bindings in
disabled asserts and does not discard an eval-mode string as a docstring.

This extends the bounded Slice 2 source/flag/target contract, not general
CPython bytecode compatibility. Slices 5–7 subsequently close cache and
reclamation, compatible function-code replacement, supported
async/module/reflection composition, and the bounded adversarial corpus.
