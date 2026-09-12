def compiled_manifest_edges():
    import pkg.child
    import stateful
    import reload_failure
    import control


import builtins
import importlib
import sys

original_import = builtins.__import__
calls = []


def import_hook(name, globals=None, locals=None, fromlist=(), level=0):
    calls.append((name, fromlist is None, level))
    return original_import(name, globals, locals, fromlist, level)


builtins.__import__ = import_hook
import pkg.child as child
builtins.__import__ = original_import
print(child.value, calls[0])

import stateful
first = stateful
print(stateful.loads)
print(importlib.reload(stateful) is first, stateful.loads)
importlib.invalidate_caches()

import control
import reload_failure
control.fail = True
try:
    importlib.reload(reload_failure)
except RuntimeError:
    print("reload failed", sys.modules["reload_failure"] is reload_failure)
print(reload_failure.ready)
