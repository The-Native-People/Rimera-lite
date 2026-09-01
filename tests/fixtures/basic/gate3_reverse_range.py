print("reverse-bytes", list(reversed(b"abc")))
print("reverse-bytearray", list(reversed(bytearray(b"abc"))))

mapping = {"a": 1, "b": 2, "c": 3}
print("reverse-dict", list(reversed(mapping)))
print("reverse-keys", list(reversed(mapping.keys())))
print("reverse-values", list(reversed(mapping.values())))
print("reverse-items", list(reversed(mapping.items())))

reverse_mapping = reversed(mapping)
print("reverse-first", next(reverse_mapping))
mapping["d"] = 4
try:
    next(reverse_mapping)
except RuntimeError:
    print("reverse-mutation")

reverse_values = reversed(mapping.values())
print("reverse-view-first", next(reverse_values))
mapping["e"] = 5
try:
    next(reverse_values)
except RuntimeError:
    print("reverse-view-mutation")

class CustomReverse:
    def __reversed__(self):
        return iter([9, 8, 7])

class SequenceFallback:
    def __len__(self):
        return 3

    def __getitem__(self, index):
        return index * 10

class BadReverse:
    def __reversed__(self):
        raise ValueError("reverse boom")

class BadLength:
    def __len__(self):
        raise KeyError("length boom")

    def __getitem__(self, index):
        return index

class BadItem:
    def __len__(self):
        return 1

    def __getitem__(self, index):
        raise ValueError("item boom")

print("reverse-custom", list(reversed(CustomReverse())))
print("reverse-fallback", list(reversed(SequenceFallback())))
try:
    reversed(BadReverse())
except ValueError:
    print("reverse-custom-error")
try:
    reversed(BadLength())
except KeyError:
    print("reverse-length-error")
bad_item_reverse = reversed(BadItem())
try:
    next(bad_item_reverse)
except ValueError:
    print("reverse-item-error")
print("reverse-list", list(reversed([1, 2, 3])))
print("reverse-tuple", list(reversed((1, 2, 3))))
print("reverse-string", list(reversed("abc")))

huge = range(10 ** 100)
print("huge-truth", bool(huge))
print("range-max-len", len(range((2 ** 63) - 1)))
try:
    len(range(2 ** 63))
except OverflowError:
    print("range-max-overflow")
try:
    len(huge)
except OverflowError:
    print("huge-len-overflow")
print("huge-index", huge[10 ** 50])
print("huge-negative-index", huge[-1])

huge_reverse = reversed(huge)
print("huge-reverse-1", next(huge_reverse))
print("huge-reverse-2", next(huge_reverse))

normal = range(2, 12, 3)
print("range-attrs", normal.start, normal.stop, normal.step)
print("range-reverse", list(reversed(normal)))
print("range-negative", list(reversed(range(10, -2, -3))))
print("range-count-index", normal.count(8), normal.index(8))
print("range-float", normal.count(8.0), normal.index(8.0))
print("range-complex", normal.count(8 + 0j), normal.index(8 + 0j))
print("range-missing-count", normal.count(8.5))
try:
    normal.index(8.5)
except ValueError:
    print("range-missing-index")

class EqualEight:
    def __eq__(self, other):
        return other == 8

needle = EqualEight()
print("range-custom-eq", normal.count(needle), normal.index(needle))

empty_a = range(0)
empty_b = range(2, 1, 3)
one_a = range(5, 6, 99)
one_b = range(5, 6, 1)
print("range-empty-equal", empty_a == empty_b, hash(empty_a) == hash(empty_b))
print("range-one-equal", one_a == one_b, hash(one_a) == hash(one_b))
print("range-distinct", range(0, 4, 2) == range(0, 5, 2))
