import weakref


def pressure():
    for index in range(700):
        transient = [str(index)] * 24


class Box:
    def __init__(self, value):
        self.value = value

    def __hash__(self):
        return self.value

    def __eq__(self, other):
        return self.value == other.value


class MutatingKey:
    target = None

    def __init__(self, value, mutate):
        self.value = value
        self.mutate = mutate

    def __hash__(self):
        return 1

    def __eq__(self, other):
        if self.mutate:
            self.target.clear()
        return self.value == other.value


class CountingKey:
    def __init__(self):
        self.hash_calls = 0

    def __hash__(self):
        self.hash_calls += 1
        return 97


first = Box(1)
second = Box(2)
keys = weakref.WeakKeyDictionary({first: "one"})
keys[second] = "two"
print("weak-keys-live", len(keys), keys[first], second in keys)
print("weak-keys-iter", sorted([item.value for item in keys]))
key_iterator = iter(keys)
next(key_iterator)
third = Box(3)
keys[third] = "three"
try:
    next(key_iterator)
except RuntimeError as error:
    print("weak-keys-mutated", type(error).__name__)
del second
pressure()
print("weak-keys-dead", len(keys), sorted([item.value for item in keys.keys()]))

equal_original = Box(7)
equal_replacement = Box(7)
equal_keys = weakref.WeakKeyDictionary({equal_original: "old"})
equal_keys[equal_replacement] = "new"
print(
    "weak-keys-equal-update",
    len(equal_keys),
    equal_keys[equal_replacement],
    next(iter(equal_keys)) is equal_original,
)
del equal_original
pressure()
print("weak-keys-equal-dead", len(equal_keys), equal_replacement in equal_keys)

iter_original = Box(8)
iter_replacement = Box(8)
iter_keys = weakref.WeakKeyDictionary({iter_original: "before"})
iter_key_items = iter_keys.items()
iter_keys[iter_replacement] = "after"
print("weak-keys-iterator-replace", next(iter_key_items)[1])

setdefault_key = CountingKey()
setdefault_keys = weakref.WeakKeyDictionary()
print(
    "weak-keys-setdefault-missing",
    setdefault_keys.setdefault(setdefault_key, "value"),
    setdefault_key.hash_calls,
    len(setdefault_keys),
)
setdefault_key.hash_calls = 0
print(
    "weak-keys-setdefault-hit",
    setdefault_keys.setdefault(setdefault_key, "other"),
    setdefault_key.hash_calls,
    len(setdefault_keys),
)

mutating = MutatingKey(1, True)
probe = MutatingKey(2, False)
mutation_keys = weakref.WeakKeyDictionary()
MutatingKey.target = mutation_keys
mutation_keys[mutating] = "value"
print("weak-keys-reentrant", probe in mutation_keys, len(mutation_keys))

value_one = Box(11)
value_two = Box(12)
values = weakref.WeakValueDictionary({"one": value_one})
values["two"] = value_two
print("weak-values-live", len(values), values["one"].value, "two" in values)
print("weak-values-items", sorted([(key, value.value) for key, value in values.items()]))
value_iterator = values.values()
del value_one
del value_two
pressure()
print("weak-values-dead", len(values), list(value_iterator))

pop_key = CountingKey()
pop_value = Box(13)
pop_values = weakref.WeakValueDictionary({pop_key: pop_value})
pop_key.hash_calls = 0
popped_value = pop_values.pop(pop_key)
print("weak-values-pop", popped_value.value, pop_key.hash_calls, len(pop_values))

value_setdefault_key = CountingKey()
value_setdefault_value = Box(131)
value_setdefaults = weakref.WeakValueDictionary()
print(
    "weak-values-setdefault-missing",
    value_setdefaults.setdefault(value_setdefault_key, value_setdefault_value)
    is value_setdefault_value,
    value_setdefault_key.hash_calls,
    len(value_setdefaults),
)
value_setdefault_key.hash_calls = 0
print(
    "weak-values-setdefault-hit",
    value_setdefaults.setdefault(value_setdefault_key, Box(132)) is value_setdefault_value,
    value_setdefault_key.hash_calls,
    len(value_setdefaults),
)

old_iter_value = Box(14)
new_iter_value = Box(15)
iter_values = weakref.WeakValueDictionary({"slot": old_iter_value})
iter_value_values = iter_values.values()
iter_value_items = iter_values.items()
iter_values["slot"] = new_iter_value
print(
    "weak-values-iterator-replace",
    next(iter_value_values) is new_iter_value,
    next(iter_value_items)[1] is new_iter_value,
)

set_first = Box(21)
set_second = Box(22)
members = weakref.WeakSet([set_first])
members.add(set_second)
print("weak-set-live", len(members), set_first in members, sorted([item.value for item in members]))
members.discard(set_first)
print("weak-set-discard", len(members), set_first in members)
set_iterator = iter(members)
del set_second
pressure()
print("weak-set-dead", len(members), list(set_iterator))

observed = Box(31)
public_ref = weakref.ref(observed)
print("weak-observers-base", weakref.getweakrefcount(observed))
observing_keys = weakref.WeakKeyDictionary({observed: "key"})
print("weak-observers-key", weakref.getweakrefcount(observed))
observing_values = weakref.WeakValueDictionary({"value": observed})
print("weak-observers-value", weakref.getweakrefcount(observed))
observing_set = weakref.WeakSet([observed])
print("weak-observers-set", weakref.getweakrefcount(observed))
print("weak-observers-public-canonical", weakref.ref(observed) is public_ref)

private_first = Box(32)
private_set = weakref.WeakSet([private_first])
print("weak-observers-private-only", weakref.getweakrefcount(private_first))
private_public_ref = weakref.ref(private_first)
print(
    "weak-observers-private-then-public",
    weakref.getweakrefcount(private_first),
    weakref.ref(private_first) is private_public_ref,
)
