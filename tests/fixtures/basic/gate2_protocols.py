class Value:
    def __init__(self, value):
        self.value = value

    def __matmul__(self, other):
        return self.value + other.value

    def __iadd__(self, other):
        self.value = self.value + other
        return self

    def __hash__(self):
        return 7

    def __str__(self):
        return "string-value"

    def __repr__(self):
        return "repr-value"

    def __format__(self, spec):
        return "format-" + spec

    def __reversed__(self):
        return [3, 2, 1]


left = Value(4)
right = Value(5)
print(left @ right)
left += 6
print(left.value)
print(hash(left))
print(str(left))
print(repr(left))
print(format(left, "ok"))
for item in reversed(left):
    print(item)

values = [1, 2, 3]
values[0] += 9
del values[1]
print(values)
print(left is left)
print(left is not right)


class Parent:
    def __add__(self, other):
        return "parent"


class Child(Parent):
    def __radd__(self, other):
        return "child"


print(Parent() + Child())


class LegacySequence:
    def __len__(self):
        return 3

    def __getitem__(self, index):
        if index >= 3:
            raise IndexError("done")
        return index + 10


for item in LegacySequence():
    print(item)
print(11 in LegacySequence())
for item in reversed(LegacySequence()):
    print(item)


class EqualityOnly:
    def __eq__(self, other):
        return True


try:
    print(hash(EqualityOnly()))
except TypeError:
    print("unhashable")
