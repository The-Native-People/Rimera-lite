class Provider:
    def __init__(self, data, events):
        self.data = bytearray(data)
        self.events = events

    def __buffer__(self, flags):
        self.events.append(("get", flags))
        self.inner = memoryview(self.data)
        return self.inner

    def __release_buffer__(self, view):
        self.events.append(("release", view.tobytes()))


events = []
p = Provider(b"abcd", events)
v = memoryview(p)
child = v[1:3]
print(v.tobytes(), child.tobytes(), v.readonly)
print(events)
v.release()
print(events, child.tobytes())
child[0] = 90
print(p.data)
child.release()
print(events)

nested_events = []
q = Provider(b"xy", nested_events)
a = memoryview(q)
b = memoryview(a)
a.release()
print(b.tobytes(), nested_events)
b.release()
print(nested_events)


class ReadOnly:
    def __buffer__(self, flags):
        return memoryview(b"readonly")


readonly = memoryview(ReadOnly())
print(readonly.readonly, readonly.tobytes())
readonly.release()


class Bad:
    def __buffer__(self, flags):
        return b"bad"


try:
    memoryview(Bad())
except TypeError:
    print("bad return rejected")


class Boom:
    def __buffer__(self, flags):
        raise ValueError("ouch")


try:
    memoryview(Boom())
except ValueError:
    print("provider error preserved")

