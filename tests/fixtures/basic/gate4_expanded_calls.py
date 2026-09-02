events = []


def mark(label, value):
    events.append(("eval", label))
    return value


class Values:
    def __init__(self, values, label):
        self.values = values
        self.label = label
        self.index = 0

    def __iter__(self):
        events.append(("iter", self.label))
        return self

    def __next__(self):
        if self.index == len(self.values):
            raise StopIteration
        value = self.values[self.index]
        events.append(("next", self.label, value))
        self.index += 1
        return value


class Keywords:
    def __init__(self, entries, label):
        self.entries = entries
        self.label = label

    def keys(self):
        events.append(("keys", self.label))
        result = []
        for entry in self.entries:
            result.append(entry[0])
        return result

    def __getitem__(self, key):
        events.append(("get", self.label, key))
        for current, value in self.entries:
            if current == key:
                return value
        raise KeyError(key)


def target(a, b, /, c=0, *rest, d=0, **kw):
    events.append(("callee", a, b, c, rest, d, kw))
    return (a, b, c, rest, d, kw)


result = target(
    mark("a", 1),
    *mark("star", Values([2, 3], "star-values")),
    d=mark("d", 4),
    **mark("kw", Keywords([("x", 5), ("y", 6)], "kw-map"))
)
print(result)
print(events)


class Holder:
    def method(self, first, *rest, flag=0, **kw):
        return (first, rest, flag, kw)


holder = Holder()
print(holder.method(*[7, 8], flag=9, **{"z": 10}))


class Callable:
    def __call__(self, first, *rest, **kw):
        return (first, rest, kw)


print(Callable()(*[11, 12], **{"k": 13}))


class Box:
    def __init__(self, first, *rest, **kw):
        self.value = (first, rest, kw)


box = Box(*[14, 15], **{"q": 16})
print(box.value)


duplicate_log = []
class Duplicate:
    def keys(self):
        duplicate_log.append("keys")
        return ["x"]

    def __getitem__(self, key):
        duplicate_log.append(("get", key))
        return 99


def duplicate_target(**kw):
    duplicate_log.append("callee")


try:
    duplicate_target(x=1, **Duplicate())
except TypeError:
    print("duplicate", duplicate_log)


bad_key_log = []
class BadKeys:
    def keys(self):
        bad_key_log.append("keys")
        return [1]

    def __getitem__(self, key):
        bad_key_log.append(("get", key))
        return 1


try:
    duplicate_target(**BadKeys())
except TypeError:
    print("bad-key", bad_key_log)


failure_log = []
class BrokenValues:
    def __iter__(self):
        failure_log.append("iter")
        return self

    def __next__(self):
        failure_log.append("next")
        raise ValueError("star exploded")


try:
    target(*BrokenValues())
except ValueError as error:
    print(str(error), failure_log)
