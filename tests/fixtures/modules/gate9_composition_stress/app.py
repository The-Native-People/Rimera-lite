def compiled_manifest_edges():
    import pkg.left
    import pkg.right
    import state


import builtins
import importlib
import sys

original_import = builtins.__import__
hook_calls = []


def hook(name, globals=None, locals=None, fromlist=(), level=0):
    hook_calls.append((name, level))
    return original_import(name, globals, locals, fromlist, level)


builtins.__import__ = hook
import pkg.left as left
builtins.__import__ = original_import
print(left.cycle_value(), hook_calls[0])

generator = left.stream()
print(next(generator))
try:
    next(generator)
except StopIteration:
    print(left.events)

import state
identity = state
index = 0
while index < 12:
    state = importlib.reload(state)
    index += 1
print(state is identity, state.loads)
print(sys.modules["state"] is state)
