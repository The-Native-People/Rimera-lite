events = []


class ExplodingIterator:
    def __init__(self):
        self.index = 0

    def __iter__(self):
        events.append("comp-iter")
        return self

    def __next__(self):
        events.append("comp-next:" + str(self.index))
        if self.index == 1:
            raise ValueError("comprehension boom")
        self.index += 1
        return 4


try:
    result = [value * 2 for value in ExplodingIterator()]
    print("bad-comprehension", result)
except ValueError as error:
    print("comprehension-error", str(error))


class BadFormat:
    def __format__(self, spec):
        events.append("format:" + spec)
        raise ValueError("format boom")


try:
    print(f"value={BadFormat():boom}")
except ValueError as error:
    print("format-error", str(error))


class RaisingDescriptor:
    def __get__(self, instance, owner):
        events.append("match-descriptor")
        raise ValueError("match boom")


class MatchBoom:
    value = RaisingDescriptor()


pattern_state = "kept"
try:
    match MatchBoom():
        case MatchBoom(value=pattern_state):
            print("bad-match")
except ValueError as error:
    print("match-error", str(error), pattern_state)


first = "first-old"
second = "second-old"
third = "third-old"
try:
    first, (second, third) = [1, [2]]
except ValueError as error:
    print("target-error", first, second, third, type(error).__name__)


capture = "capture-old"
match [1, [2, 99]]:
    case [capture, [inner, 3]]:
        print("bad-capture")
    case _:
        print("pattern-rollback", capture)


class SliceFailure:
    def __getitem__(self, key):
        events.append(("slice-failure", key.start, key.stop, key.step))
        raise RuntimeError("slice boom")


try:
    SliceFailure()[1:5:2]
except RuntimeError as error:
    print("slice-error", str(error))

print("events", events)
