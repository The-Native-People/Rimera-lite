events = []


def mark(label, value):
    events.append(label)
    return value


x: mark("x-ann", int) = mark("x-value", 3)
print(x)
print(events)
print(__annotations__)

events.clear()


def func(
    a: mark("a-ann", int) = mark("a-default", 2),
    *,
    b: mark("b-ann", str) = mark("b-default", "x"),
) -> mark("ret-ann", str):
    local_only: mark("local-ann", int)
    local_value: mark("local-val-ann", int) = mark("local-value", 7)
    return a, b, local_value


print(events)
print(func.__annotations__)
print(func())
print(events)

events.clear()


class C:
    y: mark("cy-ann", int) = mark("cy-value", 4)
    z: mark("cz-ann", str)


print(events)
print(C.y)
print(C.__annotations__)
func.__annotations__ = None
print(func.__annotations__)
