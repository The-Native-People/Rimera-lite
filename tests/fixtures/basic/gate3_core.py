print(ascii("café\n"))
print(repr("a'b"), repr('a"b'), repr("\x01"), repr(b"a'b"))

class Render:
    def __repr__(self):
        return "R<é>"

    def __format__(self, spec):
        return "fmt:" + spec

render = Render()
print(repr([render, {"x": render}]))
print(ascii(render))
print(format("xy", ">5"))
print(format(42, "#06x"), format(12345, ",d"))
print(format(3.5, ".2f"), format(3.5, "08.2f"))
print(format(render, "ok"))

class DirectDivmod:
    def __divmod__(self, other):
        return ("direct", other)

class ReflectedDivmod:
    def __rdivmod__(self, other):
        return ("reflected", other)

print(divmod(DirectDivmod(), 7))
print(divmod(7, ReflectedDivmod()))

class TernaryPower:
    def __pow__(self, exponent, modulus=None):
        return (exponent, modulus)

print(pow(TernaryPower(), 3, 5))
print(pow(2, -1, 5), pow(2, 5, -7))

values = [3, 1, 2]
values.append(4)
values.extend([5, 6])
values.insert(0, 0)
print(values.pop(), values.count(2), values.index(3))
values.remove(4)
values.reverse()
values.sort()
print(values, values.copy())

mapping = {"a": 1}
print(mapping.get("a"), mapping.get("x", 9))
print(mapping.setdefault("b", 2), mapping.setdefault("b", 8))
mapping.update({"c": 3}, d=4)
print(list(mapping.items()))
print(mapping.pop("c"), mapping.pop("x", 7))
copy = mapping.copy()
print(copy == mapping)
print(mapping.popitem())
mapping.clear()
print(mapping)

left = {1, 2, 3}
right = frozenset({3, 4})
print(sorted(left | right), sorted(left & right), sorted(left - right), sorted(left ^ right))
print(sorted(right | left))
print(left.isdisjoint({8}), left.issubset({1, 2, 3, 4}), left.issuperset({2}))
extra = left.copy()
extra.update([4, 5])
extra.discard(5)
extra.remove(4)
print(sorted(extra))
popped = extra.pop()
print(popped in {1, 2, 3}, len(extra))
extra.clear()
print(extra)

view_mapping = {"a": 1}
keys = view_mapping.keys()
view_mapping["b"] = 2
print(sorted(keys), keys.isdisjoint({"z"}), sorted(keys | {"c"}))

recursive = []
recursive.append(recursive)
print(repr(recursive))
