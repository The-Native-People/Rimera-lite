events = []


class Box:
    def __init__(self, value):
        self.value = value


box = Box(10)
items = [20, 30]


def receiver():
    events.append("receiver")
    return box


def collection():
    events.append("collection")
    return items


def index():
    events.append("index")
    return 0


def rhs(label, value):
    events.append(label)
    return value


receiver().value += rhs("attr-rhs", 5)
collection()[index()] *= rhs("item-rhs", 2)
print(box.value, items, events)

first = 1
second = 2
del (first, second)
try:
    print(first)
except NameError:
    print("first gone")
try:
    print(second)
except NameError:
    print("second gone")

holder = Box(99)
values = [1, 2, 3]
del holder.value, values[1]
print(hasattr(holder, "value"), values)

message_events = []


def message():
    message_events.append("message")
    return "assertion message"


assert True, message()
print(message_events)
try:
    assert False, message()
except AssertionError as error:
    print(type(error).__name__, str(error), message_events)


class Truth:
    def __init__(self, value):
        self.value = value

    def __bool__(self):
        message_events.append("truth")
        return self.value


try:
    try:
        assert Truth(False), "nested"
    finally:
        print("assert finally")
except AssertionError as error:
    print(type(error).__name__, str(error), message_events)


class ClassBody:
    number = 3
    number += 4
    box = Box(5)
    box.value *= 2
    items = [1, 2, 3]
    items[0] += 9
    doomed = 1
    also_doomed = 2
    del (doomed, also_doomed)
    del items[1]
    assert number == 7, "class number"
    assert box.value == 10


print(ClassBody.number, ClassBody.box.value, ClassBody.items)
print(hasattr(ClassBody, "doomed"), hasattr(ClassBody, "also_doomed"))
