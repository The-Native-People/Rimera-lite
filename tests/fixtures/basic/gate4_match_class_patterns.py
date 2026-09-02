events = []

class Point:
    __match_args__ = ("x", "y")

    def __init__(self, x, y):
        self.x = x
        self.y = y

class Child(Point):
    pass

point = Point(3, 4)
match point:
    case Point(x, y):
        print("positional", x, y)

match point:
    case Point(y=right, x=left):
        print("keyword", left, right)

child = Child(5, 6)
match child:
    case Point(cx, cy):
        print("inheritance", cx, cy)

class Logged:
    def __get__(self, instance, owner):
        events.append("descriptor")
        if instance is None:
            return self
        return instance._value

class WithDescriptor:
    __match_args__ = ("value",)
    value = Logged()

    def __init__(self, value):
        self._value = value

match WithDescriptor(7):
    case WithDescriptor(found):
        print("descriptor", found)

class Missing:
    pass

match Missing():
    case Missing(value=missing):
        print("bad-missing")
    case _:
        print("missing")

class Slotted:
    __slots__ = ("x",)
    __match_args__ = ("x",)

    def __init__(self, x):
        self.x = x

match Slotted(8):
    case Slotted(slot_value):
        print("slot", slot_value)

match 9:
    case int(integer_value):
        print("builtin", integer_value)

class Meta(type):
    __match_args__ = ("meta_value",)

class FromMeta(metaclass=Meta):
    def __init__(self, value):
        self.meta_value = value

match FromMeta(10):
    case FromMeta(meta_value):
        print("metaclass", meta_value)

match {"node": Point(11, 12)}:
    case {"node": Point(11, nested_y)}:
        print("nested", nested_y)

kept = "original"
match [Point(1, 99)]:
    case [Point(kept, 2)]:
        print("bad-rollback")
    case _:
        print("rollback", kept)

class BadShape:
    __match_args__ = ["x"]
    x = 1

try:
    match BadShape():
        case BadShape(value):
            print("bad-shape")
except TypeError:
    print("invalid-shape")

class BadEntry:
    __match_args__ = (1,)
    x = 1

try:
    match BadEntry():
        case BadEntry(value):
            print("bad-entry")
except TypeError:
    print("invalid-entry")

try:
    match Point(1, 2):
        case Point(a, b, c):
            print("bad-count")
except TypeError:
    print("too-many")

try:
    match Point(1, 2):
        case Point(a, x=b):
            print("bad-duplicate")
except TypeError:
    print("duplicate-attribute")

class Values:
    not_type = 42

try:
    match point:
        case Values.not_type():
            print("bad-non-type")
except TypeError:
    print("non-type")

class Raising:
    def __get__(self, instance, owner):
        events.append("raising")
        raise ValueError("descriptor boom")

class Boom:
    boom = Raising()

try:
    match Boom():
        case Boom(boom=value):
            print("bad-boom")
except ValueError:
    print("descriptor-error")

print(events)
