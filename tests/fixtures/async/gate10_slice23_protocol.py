events = []

async def leaf(value):
    events.append("leaf")
    return value + 1

async def parent(value):
    events.append("parent-start")
    result = await leaf(value)
    events.append("parent-end")
    return result + 1

async def fail_leaf():
    raise ValueError("boom")

async def fail_parent():
    await fail_leaf()

async def needs_arg(value):
    return value

coro = parent(40)
print(events)
print(coro.cr_frame is None, coro.cr_running, coro.cr_suspended, coro.cr_await)
try:
    coro.send(None)
except StopIteration as exc:
    print("return", exc.value)
print(events)
print(coro.cr_frame is None, coro.cr_running, coro.cr_suspended, coro.cr_await)
try:
    coro.send(None)
except RuntimeError as exc:
    print(type(exc).__name__, str(exc))

failure = fail_parent()
try:
    failure.send(None)
except ValueError as exc:
    print(type(exc).__name__, str(exc))

closed = parent(1)
print(closed.cr_frame is None)
closed.close()
print(closed.cr_frame is None, events)

try:
    needs_arg()
except TypeError as exc:
    print(type(exc).__name__, str(exc))
