def compiled_manifest_edges():
    import event_log
    import counter
    import pkg.child
    import mutator


import sys
import event_log

print(sys.modules is sys.modules)

counter_first = __import__("counter")
print(counter_first.value, sys.modules["counter"] is counter_first)
del sys.modules["counter"]
counter_second = __import__(name="counter")
print(counter_second.value, counter_second is counter_first)

leaf = __import__("pkg.child", fromlist=("value",))
print(leaf.value)
relative = __import__("child", {"__package__": "pkg"}, {}, ("value",), 1)
print(relative is leaf)

replacement = {"kind": "replacement"}
sys.modules["counter"] = replacement
print(__import__("counter") is replacement)

mutated = __import__("mutator")
print(mutated["kind"])

sys.modules["blocked"] = None
try:
    __import__("blocked")
except ModuleNotFoundError:
    print("blocked")

print(event_log.events)

try:
    __import__()
except TypeError:
    print("missing-name")

try:
    __import__(1)
except TypeError:
    print("non-string-name")

try:
    __import__("counter", level=-1)
except ValueError:
    print("negative-level")

try:
    __import__("counter", level=2**100)
except OverflowError:
    print("overflow-level")

try:
    __import__("child", 1, {}, (), 1)
except TypeError:
    print("bad-globals")

try:
    __import__("counter", unexpected=True)
except TypeError:
    print("bad-keyword")

try:
    __import__("counter", name="counter")
except TypeError:
    print("duplicate-name")

print(__import__("sys", fromlist=1) is sys)

try:
    __import__("pkg", fromlist=(1,))
except TypeError:
    print("bad-fromlist-item")
