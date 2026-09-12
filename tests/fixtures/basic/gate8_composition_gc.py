events = []


class ExitDescriptor:
    def __get__(self, obj, owner=None):
        name = obj.name
        events.append("bind:" + name)

        def exit(exc_type, exc, traceback):
            if exc_type is None:
                exception_name = "none"
            else:
                exception_name = exc_type.__name__
            events.append(("exit", name, exception_name))
            return obj.suppress

        return exit


class Manager:
    __exit__ = ExitDescriptor()

    def __init__(self, name, value, suppress=False):
        self.name = name
        self.value = value
        self.suppress = suppress
        self.cycle = self

    def __enter__(self):
        events.append("enter:" + self.name)
        return self.value


def build(seed):
    values = []
    with Manager("outer", (seed, seed + 1, seed + 2)) as (first, *middle, last):
        values = [first * 2, middle[0] * 2, last * 2]
        mapping = {str(item): item for item in values}
        match values:
            case [head, *tail]:
                events.append(("match", head, tail))
        with Manager("inner", mapping, suppress=True) as entered:
            events.append(("mapping", entered[str(values[0])]))
            raise ValueError("suppressed")
    return values


print(build(3))
print(events)

events.clear()

with Manager(*("expanded", 12), **{"suppress": False}) as expanded:
    local_view = locals()
    events.append(("expanded", expanded, local_view["expanded"], "events" in globals()))
print(events)

events.clear()
with Manager("group", None, suppress=True):
    raise ExceptionGroup("suppressed group", (ValueError("left"), TypeError("right")))
events.append("group-continued")
print(events)

events.clear()


class Provider:
    def __init__(self):
        self.data = bytearray(b"context")

    def __buffer__(self, flags):
        events.append(("buffer", flags))
        self.view = memoryview(self.data)
        return self.view

    def __release_buffer__(self, view):
        events.append(("release", view.tobytes()))


with Manager("buffer", None):
    view = memoryview(Provider())
    child = view[1:4]
    events.append(child.tobytes())
    child.release()
    view.release()
print(events)

events.clear()


def stream():
    with Manager("stream", [10, 20, 30]) as values:
        yield from (value + 1 for value in values)


generator = stream()
print(next(generator), next(generator), next(generator))
try:
    next(generator)
except StopIteration:
    print("stream-done")
print(events)

events.clear()
for index in range(180):
    with Manager("gc", index):
        payload = [index, {"value": index}, (index, index + 1)]
        if index == 179:
            events.append(payload[1]["value"])
    if index != 179:
        events.clear()
print(events[-3:])
