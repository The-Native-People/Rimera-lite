import weakref


def pressure():
    for index in range(600):
        transient = [str(index)] * 24


class Item:
    def __init__(self, value):
        self.value = value

    def __eq__(self, other):
        return self.value == other.value

    def __hash__(self):
        return self.value + 31

    def __len__(self):
        return self.value

    def __getitem__(self, index):
        return self.value + index

    def __add__(self, other):
        return self.value + other


class Callback:
    def __init__(self, label, output):
        self.label = label
        self.output = output

    def __call__(self, reference):
        self.output.append(self.label)
        pressure()


class ClosedSlots:
    __slots__ = ("value",)

    def __init__(self):
        self.value = 1


class WeakSlots:
    __slots__ = ("value", "__weakref__")

    def __init__(self):
        self.value = 1


target = Item(11)
print("weak-slot-empty", target.__weakref__ is None)
plain = weakref.ref(target)
same = weakref.ref(target)
print(
    "live",
    plain() is target,
    plain is same,
    plain.__callback__ is None,
    target.__weakref__ is plain,
)
print("types", type(plain) is weakref.ReferenceType)
print("hash-live", hash(plain), hash(target))

equal_target = Item(11)
equal_ref = weakref.ref(equal_target)
print("equal-live", plain == equal_ref)

callbacks = []
ordered = [
    weakref.ref(target, Callback("first", callbacks)),
    weakref.ref(target, Callback("second", callbacks)),
    weakref.ref(target, Callback("third", callbacks)),
]
print("weak-slot-canonical", target.__weakref__ is plain)
print("observed", weakref.getweakrefcount(target), len(weakref.getweakrefs(target)))
del target
pressure()
print("dead", plain() is None, hash(plain), callbacks)
print("equal-dead", plain == equal_ref, plain == plain)

uncached_target = Item(3)
uncached = weakref.ref(uncached_target)
del uncached_target
pressure()
try:
    hash(uncached)
except Exception as error:
    print("dead-hash", type(error).__name__, str(error))

proxy_target = Item(5)
proxy = weakref.proxy(proxy_target)
proxy.value = 8
print("proxy-live", proxy.value, len(proxy), proxy[2], proxy + 3, type(proxy).__name__)
del proxy_target
pressure()
try:
    print(proxy.value)
except Exception as error:
    print("proxy-dead", type(error).__name__, str(error))

callable_target = Callback("called", callbacks)
callable_reference = weakref.ref(callable_target)
callable_proxy = weakref.proxy(callable_target)
print(
    "callable",
    callable(callable_reference),
    callable(callable_proxy),
    type(callable_proxy) is weakref.CallableProxyType,
)
callable_proxy(callable_reference)
print("called", callbacks[-1])

try:
    weakref.ref([])
except Exception as error:
    print("not-weakrefable", type(error).__name__, str(error))

try:
    weakref.ref(ClosedSlots())
except Exception as error:
    print("closed-slots", type(error).__name__)
weak_slots = WeakSlots()
print("weak-slots", weakref.ref(weak_slots)() is weak_slots)
