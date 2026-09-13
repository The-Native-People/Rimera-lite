import weakref


def pressure():
    for index in range(700):
        transient = [str(index)] * 24


events = []


class Finalized:
    def __init__(self, label):
        self.label = label

    def __del__(self):
        events.append("del:" + self.label)


ordinary = Finalized("ordinary")
ordinary_ref = weakref.ref(ordinary, lambda reference: events.append("weak:ordinary"))
del ordinary
pressure()
print("ordinary-order", events, ordinary_ref() is None)
pressure()
print("ordinary-once", events)


class BaseFinalized:
    def __del__(self):
        events.append("del:inherited")


class ChildFinalized(BaseFinalized):
    pass


inherited = ChildFinalized()
del inherited
pressure()
print("inherited", events[-1])


class DynamicFinalized:
    pass


def dynamic_del(self):
    events.append("del:dynamic")


DynamicFinalized.__del__ = dynamic_del
dynamic = DynamicFinalized()
del dynamic
pressure()
print("dynamic", events[-1])


class BrokenFinalized:
    def __del__(self):
        events.append("del:broken")
        raise RuntimeError("finalizer boom")


broken = BrokenFinalized()
del broken
pressure()
print("finalizer-exception-survived", events[-1])


def broken_callback(reference):
    raise ValueError("weak callback boom")


callback_target = Finalized("callback-target")
callback_ref = weakref.ref(callback_target, broken_callback)
del callback_target
pressure()
print("weak-callback-exception-survived", callback_ref() is None)


class ShutdownFinalized:
    def __init__(self, label):
        self.label = label

    def __del__(self):
        print("shutdown", self.label)


shutdown_a = ShutdownFinalized("a")
shutdown_b = ShutdownFinalized("b")
print("body-end")
