events = []


def allocate(label):
    junk = None
    for ignored in range(80):
        junk = "xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"
    events.append(label)


def pack(a, b, *, label):
    allocate("pack:" + label)
    return label + ":" + str(a + b)


class Formatted:
    def __init__(self, value):
        self.value = value

    def __format__(self, spec):
        allocate("format:" + str(self.value))
        return str(self.value) + spec


pairs = [(1, 2), (3, 4)]
rows = [
    pack(*pair, label=f"{Formatted(pair[0]):!}")
    for pair in pairs
    if (selected := pair[0]) > 0
]
print("rows", rows, selected)


class HeavyDescriptor:
    def __get__(self, instance, owner):
        allocate("descriptor")
        if instance is None:
            return self
        return instance._value


class Node:
    __match_args__ = ("items", "value")
    value = HeavyDescriptor()

    def __init__(self, items, value):
        self.items = items
        self._value = value


captured = "old"
match {"node": Node([8, 9, 10, 11], "alive")}:
    case {"node": Node([head, *tail], captured)} if (guarded := len(tail)):
        first, *rest = tail
        print("match", head, tail, captured, guarded, first, rest)
    case _:
        print("bad")

print("events", events)
