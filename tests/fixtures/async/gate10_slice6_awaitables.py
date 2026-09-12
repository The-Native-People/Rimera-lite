events = []

class Awaitable:
    def __init__(self, base):
        self.base = base

    def __await__(self):
        events.append("await-start")
        incoming = yield "token"
        events.append(incoming)
        return self.base + incoming

async def consume():
    return await Awaitable(40)

coro = consume()
print(coro.send(None))
try:
    coro.send(2)
except StopIteration as exc:
    print("return", exc.value)
print(events)

class Missing:
    pass

async def missing():
    return await Missing()

bad = missing()
try:
    bad.send(None)
except TypeError as exc:
    print(type(exc).__name__, str(exc))

class NonIterator:
    def __await__(self):
        return 3

async def non_iterator():
    return await NonIterator()

bad = non_iterator()
try:
    bad.send(None)
except TypeError as exc:
    print(type(exc).__name__, str(exc))

class ThrowAware:
    def __await__(self):
        try:
            yield "throw-token"
        except ValueError as exc:
            events.append("caught:" + str(exc))
            return 7
        finally:
            events.append("cleanup")

async def throwing():
    return await ThrowAware()

thrown = throwing()
print(thrown.send(None))
try:
    thrown.throw(ValueError("boom"))
except StopIteration as exc:
    print("throw-return", exc.value)
print(events)

events.clear()
closed = throwing()
print(closed.send(None))
closed.close()
print(events)

class MutableAwaitable:
    def __await__(self):
        yield "old-token"
        return 10

async def mutable_once():
    return await MutableAwaitable()

mutable = mutable_once()
print(mutable.send(None))
try:
    mutable.send(None)
except StopIteration as exc:
    print("old-return", exc.value)

def replacement_await(self):
    yield "new-token"
    return 20

MutableAwaitable.__await__ = replacement_await
mutable = mutable_once()
print(mutable.send(None))
try:
    mutable.send(None)
except StopIteration as exc:
    print("new-return", exc.value)

cycle = Awaitable(5)
cycle.self = cycle

async def cycle_path():
    return await cycle

cycled = cycle_path()
print(cycled.send(None))
try:
    cycled.send(3)
except StopIteration as exc:
    print("cycle-return", exc.value)
