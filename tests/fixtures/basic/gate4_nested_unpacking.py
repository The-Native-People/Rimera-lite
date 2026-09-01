events = []


class OneShot:
    def __init__(self, values):
        self.values = values
        self.index = 0

    def __iter__(self):
        return self

    def __next__(self):
        if self.index >= len(self.values):
            raise StopIteration
        value = self.values[self.index]
        self.index += 1
        return value


class Holder:
    def __init__(self):
        self.value = 0


holder = Holder()
slots = [0]


def holder_target():
    events.append("holder")
    return holder


def slots_target():
    events.append("slots")
    return slots


def index_target():
    events.append("index")
    return 0


first, (holder_target().value, slots_target()[index_target()]) = OneShot(
    [10, OneShot([20, 30])]
)
print(first, holder.value, slots, events)

first = "old-first"
middle = "old-middle"
last = "old-last"
try:
    first, (middle, last) = OneShot([1, OneShot([2])])
except ValueError as error:
    print(type(error).__name__, str(error))
print(first, middle, last)

outer = "old-outer"
other = "old-other"
try:
    outer, other = OneShot([9])
except ValueError as error:
    print(type(error).__name__, str(error))
print(outer, other)


class Failing:
    def __iter__(self):
        return self

    def __next__(self):
        raise RuntimeError("iterator boom")


try:
    failed_left, failed_right = Failing()
except RuntimeError as error:
    print(type(error).__name__, str(error))
