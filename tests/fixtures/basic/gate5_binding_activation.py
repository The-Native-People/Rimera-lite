events = []


def target(a, b=2, /, c=3, *args, d, e=5, **kwargs):
    events.append(("enter", a, b, c, args, d, e, kwargs))
    return a, b, c, args, d, e, kwargs


print(target(1, 4, 6, 7, 8, d=9, z=10))
print(target(1, d=4, a=99))


def mark(label, value):
    events.append(label)
    return value


print(
    target(
        mark("p1", 10),
        *mark("star", [11, 12]),
        d=mark("kw", 13),
        **mark("map", {"extra": 14}),
    )
)
print(events)


class Callable:
    def __call__(self, value, *, flag=20):
        return "callable", value, flag


class Holder:
    def method(self, value, *, flag=30):
        return "method", value, flag


class Constructed:
    def __init__(self, value, *, flag=40):
        self.value = value
        self.flag = flag


print(Callable()(1, flag=2))
print(Holder().method(3, flag=4))
constructed = Constructed(5, flag=6)
print(constructed.value, constructed.flag)


def decorate(function):
    def wrapper(*args, **kwargs):
        return "decorated", function(*args, **kwargs)

    return wrapper


@decorate
def decorated(value, *, flag=50):
    return value, flag


print(decorated(7, flag=8))


def recursive(depth, marker):
    local = [depth, marker]
    if depth == 0:
        return local
    child = recursive(depth - 1, marker + 1)
    return local, child


def left(depth):
    here = ("left", depth)
    if depth == 0:
        return here
    return here, right(depth - 1)


def right(depth):
    here = ("right", depth)
    if depth == 0:
        return here
    return here, left(depth - 1)


print(recursive(3, 10))
print(left(3))


class ReentrantIterable:
    def __iter__(self):
        events.append(("iter", recursive(1, 70)))
        return iter([21, 22])


print(target(20, *ReentrantIterable(), d=23))


entered = []


def must_not_enter(*args, **kwargs):
    entered.append((args, kwargs))


class BrokenIterable:
    def __iter__(self):
        events.append("broken-iter")
        raise ValueError("expand boom")


try:
    must_not_enter(1, *BrokenIterable(), named=2)
except ValueError as error:
    print(type(error).__name__, str(error), entered)


for call in [
    lambda: target(d=1),
    lambda: target(1, 2, 3, d=4, c=5),
    lambda: target(1, unknown=2, d=3),
]:
    try:
        call()
    except TypeError as error:
        print(type(error).__name__, str(error))


try:
    1 // 0
except ZeroDivisionError:
    def reraiser():
        raise

    try:
        reraiser()
    except Exception as error:
        print(type(error).__name__, str(error))

print(events[-2:])
