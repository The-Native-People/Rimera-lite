events = []


class Manager:
    def __init__(self, name, suppress=False):
        self.name = name
        self.suppress = suppress

    def __enter__(self):
        events.append("enter:" + self.name)
        return self

    def __exit__(self, exc_type, exc, traceback):
        if exc_type is None:
            name = "none"
        else:
            name = exc_type.__name__
        events.append(("exit", self.name, name, traceback is None))
        return self.suppress


def normal():
    with Manager("normal"):
        received = yield "ready"
        events.append(("received", received))
        yield "again"
    return 9


generator = normal()
print(next(generator))
print(generator.send(4))
try:
    next(generator)
except StopIteration as error:
    print(error.value)
print(events)

events.clear()


def suppress_throw():
    with Manager("throw", suppress=True):
        yield "open"
    yield "after"


generator = suppress_throw()
print(next(generator))
print(generator.throw(ValueError("injected")))
try:
    next(generator)
except StopIteration:
    print("done")
print(events)

events.clear()
injected = ValueError("visible")


def propagate_throw():
    with Manager("propagate"):
        yield "open"


generator = propagate_throw()
print(next(generator))
try:
    generator.throw(injected)
except ValueError as error:
    print(error is injected)
print(events)

events.clear()


def close_scope(suppress):
    with Manager("close", suppress=suppress):
        yield "open"


generator = close_scope(False)
print(next(generator))
print(generator.close())
print(events)

events.clear()
generator = close_scope(True)
print(next(generator))
print(generator.close())
print(events)

events.clear()


def ignores_close():
    with Manager("ignored-close", suppress=True):
        yield "open"
    yield "ignored"


generator = ignores_close()
print(next(generator))
try:
    generator.close()
except RuntimeError as error:
    print(type(error).__name__, str(error))
print(events)

events.clear()


class OldExit:
    def __get__(self, obj, owner=None):
        events.append("capture-old")

        def old(exc_type, exc, traceback):
            events.append("call-old")
            return False

        return old


class NewExit:
    def __get__(self, obj, owner=None):
        events.append("capture-new")

        def new(exc_type, exc, traceback):
            events.append("call-new")
            return False

        return new


class MutableManager:
    __exit__ = OldExit()

    def __enter__(self):
        return self


def captured_exit():
    with MutableManager():
        yield "pause"


generator = captured_exit()
print(next(generator))
MutableManager.__exit__ = NewExit()
try:
    next(generator)
except StopIteration:
    print("captured-done")
print(events)

events.clear()


class Delegate:
    def __init__(self):
        self.step = 0

    def __iter__(self):
        return self

    def __next__(self):
        self.step += 1
        if self.step == 1:
            return "delegated"
        raise StopIteration("delegate-result")

    def throw(self, error):
        events.append(("delegate-throw", type(error).__name__))
        raise StopIteration("throw-result")

    def close(self):
        events.append("delegate-close")


def delegated():
    with Manager("delegate"):
        result = yield from Delegate()
        events.append(("delegate-result", result))
        yield "tail"


generator = delegated()
print(next(generator))
print(generator.throw(ValueError("delegated-error")))
try:
    next(generator)
except StopIteration:
    print("delegate-done")
print(events)

events.clear()
generator = delegated()
print(next(generator))
print(generator.close())
print(events)

events.clear()


def abandoned():
    with Manager("abandoned"):
        yield "open"


generator = abandoned()
print(next(generator))
generator = None
for index in range(600):
    garbage = [
        index,
        index,
        index,
        index,
        index,
        index,
        index,
        index,
        index,
        index,
        {"index": index},
    ]
print(events)
