events = []


def mark(value):
    events.append(value)
    return value


name = "hé"
width = 6
print(f"name={name!r:>{width}}")
value = 7
print(f"{value=}")
print(f"{{escaped}} {42:04d}")
print(f"{mark('first')}:{mark('second')}")
print(events)


class Formatted:
    def __format__(self, spec):
        return "fmt:" + spec

    def __repr__(self):
        return "repr-value"

    def __str__(self):
        return "str-value"


obj = Formatted()
print(f"{obj:>5}")
print(f"{obj!r}")
print(f"{obj!s}")
print(f"{name!a}")
precision = 3
print(f"{12.3456:.{precision}f}")
