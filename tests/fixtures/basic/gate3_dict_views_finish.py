mapping = {"a": 1, "b": 2}
keys = mapping.keys()
values = mapping.values()
items = mapping.items()

print("initial", list(keys), list(values), list(items))
print("reverse", list(reversed(keys)), list(reversed(values)), list(reversed(items)))

proxy = keys.mapping
print("mapping", type(proxy).__name__, repr(proxy), proxy == mapping, mapping == proxy)
print("mapping-read", list(proxy), proxy["a"], len(proxy), bool(proxy))
mapping["c"] = 3
print("live-insert", list(keys), list(values), list(items), list(proxy))
del mapping["a"]
print("live-delete", list(keys), list(values), list(items), list(proxy))
mapping["b"] = 20
print("live-replace", list(values), list(items), repr(proxy))

print("view-sets", sorted(keys | {"z"}), sorted(keys & {"b", "x"}), sorted(keys - {"c"}), sorted(keys ^ {"b", "z"}))
print("view-compare", keys == {"b", "c"}, keys <= {"b", "c", "z"}, items == {("b", 20), ("c", 3)})
print("values-identity", values == values, mapping.values() == mapping.values())

recursive_mapping = {}
recursive_values = recursive_mapping.values()
recursive_mapping["self"] = recursive_values
print("recursive", repr(recursive_values))

forward = iter(keys)
print("forward-first", next(forward))
mapping["d"] = 4
try:
    next(forward)
except RuntimeError:
    print("forward-mutation")

reverse = reversed(items)
print("reverse-first", next(reverse))
mapping["e"] = 5
try:
    next(reverse)
except RuntimeError:
    print("reverse-mutation")

try:
    proxy["x"] = 9
except TypeError:
    print("mapping-readonly")

mapping.clear()
print("live-clear", list(keys), list(values), list(items), list(proxy), bool(proxy))
