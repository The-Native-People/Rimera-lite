events = []


def combine(a, b, *, label):
    events.append(("call", a, b, label))
    return f"{label}:{a + b}"


pairs = [(1, 2), (3, 4)]
rows = [
    combine(*pair, label=f"{pair[0]}-{pair[1]}")
    for pair in pairs
    if (filter_value := pair[0]) > 0
]
print("rows", rows, filter_value)
print(f"inline={combine(*[5, 6], label=f'{5}-{6}')}")

nested_source = [(1, (2, 3, 4)), (5, (6, 7))]
print("comp-unpack", [left + middle + len(tail) for left, (middle, *tail) in nested_source])

loop_values = []
for left, (middle, *tail) in nested_source:
    loop_values.append((left, middle, tail))
print("loop-unpack", loop_values)


class LoggedValue:
    def __get__(self, instance, owner):
        events.append(("descriptor", "get"))
        if instance is None:
            return self
        return instance._value


class Node:
    __match_args__ = ("items", "value")
    value = LoggedValue()

    def __init__(self, items, value):
        self.items = items
        self._value = value


class Holder:
    target = Node


guarded = 0
match {"node": Node([8, 9, 10], "alive")}:
    case {"node": Holder.target([head, *tail], label)} if (guarded := head + len(tail)) and label:
        body_first, *body_tail = tail
        print("match", head, tail, label, guarded, body_first, body_tail)
    case _:
        print("bad-match")


class Feed:
    def __init__(self, values):
        self.values = values
        self.index = 0

    def __iter__(self):
        events.append(("feed", "iter"))
        return self

    def __next__(self):
        if self.index == len(self.values):
            events.append(("feed", "stop"))
            raise StopIteration
        value = self.values[self.index]
        self.index += 1
        events.append(("feed", "next", value))
        return value


class Counter:
    def __init__(self):
        self.value = 0


class Bucket:
    def __init__(self):
        self.values = [10, 20, 30]

    def __getitem__(self, index):
        events.append(("bucket", "get", index))
        return self.values[index]

    def __setitem__(self, index, value):
        events.append(("bucket", "set", index, value))
        self.values[index] = value

    def __delitem__(self, index):
        events.append(("bucket", "del", index))
        del self.values[index]


counter = Counter()
bucket = Bucket()
for step in Feed([1, 2]):
    counter.value += step
    bucket[0] += step
    if step == 2:
        del bucket[1]
print("mutations", counter.value, bucket.values)


class SliceRecorder:
    def __getitem__(self, key):
        events.append(("slice", key.start, key.stop, key.step))
        return (key.start, key.stop, key.step)


slice_recorder = SliceRecorder()
print("slice", slice_recorder[1:7:2])
print("events", events)
