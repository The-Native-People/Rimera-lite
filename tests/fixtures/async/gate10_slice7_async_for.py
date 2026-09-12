events = []

class Counter:
    def __init__(self, limit):
        self.limit = limit
        self.index = 0

    def __aiter__(self):
        events.append("aiter")
        return self

    async def __anext__(self):
        events.append(self.index)
        if self.index >= self.limit:
            raise StopAsyncIteration
        value = self.index
        self.index += 1
        return value

async def break_loop():
    out = []
    async for value in Counter(5):
        if value == 1:
            continue
        out.append(value)
        if value == 2:
            break
    else:
        out.append(99)
    return out

async def exhausted_loop():
    out = []
    async for value in Counter(2):
        out.append(value)
    else:
        out.append(99)
    return out

async def builtins_path():
    iterator = aiter(Counter(1))
    return await anext(iterator)

async def default_path():
    iterator = aiter(Counter(0))
    wrapped = anext(iterator, 77)
    value = await wrapped
    try:
        await wrapped
    except RuntimeError as exc:
        return value, type(wrapped).__name__, str(exc)

for function in [break_loop, exhausted_loop, builtins_path, default_path]:
    coroutine = function()
    try:
        coroutine.send(None)
    except StopIteration as exc:
        print(exc.value)

print(events)

class Missing:
    pass

async def missing():
    async for value in Missing():
        print(value)

coroutine = missing()
try:
    coroutine.send(None)
except TypeError as exc:
    print(type(exc).__name__, str(exc))

class BadIter:
    def __aiter__(self):
        return 3

async def bad_iter():
    async for value in BadIter():
        print(value)

coroutine = bad_iter()
try:
    coroutine.send(None)
except TypeError as exc:
    print(type(exc).__name__, str(exc))

class BadNext:
    def __aiter__(self):
        return self

    def __anext__(self):
        return 3

async def bad_next():
    async for value in BadNext():
        print(value)

coroutine = bad_next()
try:
    coroutine.send(None)
except TypeError as exc:
    print(type(exc).__name__, str(exc))

injected_events = []

class Pause:
    def __await__(self):
        try:
            yield "pause-token"
        finally:
            injected_events.append("await-cleanup")

class Slow:
    def __aiter__(self):
        return self

    async def __anext__(self):
        await Pause()
        return 1

async def injected_loop():
    try:
        async for value in Slow():
            print(value)
    finally:
        injected_events.append("loop-cleanup")

coroutine = injected_loop()
print(coroutine.send(None))
try:
    coroutine.throw(BaseException("cancel"))
except BaseException as exc:
    print(type(exc).__name__, str(exc))
print(injected_events)

async def large_loop():
    total = 0
    async for value in Counter(1000):
        total += value
    return total

coroutine = large_loop()
try:
    coroutine.send(None)
except StopIteration as exc:
    print("large", exc.value)
