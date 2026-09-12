events = []

class Pause:
    def __await__(self):
        value = yield "pause-token"
        return value

async def basic():
    try:
        sent = yield 1
        resumed = await Pause()
        try:
            yield sent + resumed
        except ValueError:
            yield 99
    finally:
        events.append("basic-finally")

generator = basic()
print(type(generator).__name__)
initial_frame = generator.ag_frame
print(
    "reflect-initial",
    generator.__name__,
    generator.__qualname__,
    generator.ag_code.co_name,
    initial_frame is not None,
    generator.ag_frame is initial_frame,
)
print(generator.__aiter__() is generator, generator.ag_running, generator.ag_await)
first = generator.__anext__()
print(type(first).__name__, first.__await__() is first, iter(first) is first)
try:
    first.send(None)
except StopIteration as exc:
    print("first", exc.value)

second = generator.asend(5)
print(
    "await-token",
    second.send(None),
    generator.ag_await is not None,
    generator.ag_running,
)
print("reflect-suspended", generator.ag_frame.f_locals.get("sent"))
try:
    second.send(7)
except StopIteration as exc:
    print("second", exc.value, generator.ag_await)

thrown = generator.athrow(ValueError("boom"))
try:
    thrown.send(None)
except StopIteration as exc:
    print("throw", exc.value)

closed = generator.aclose()
try:
    closed.send(None)
except StopIteration as exc:
    print("close", exc.value)
print(events)
print("reflect-closed", generator.ag_frame)

for label, operation in [("reuse-first", first), ("reuse-second", second), ("reuse-throw", thrown), ("reuse-close", closed)]:
    try:
        operation.send(None)
    except BaseException as exc:
        print(label, type(exc).__name__, str(exc))

async def first_send_rules():
    yield 41

fresh = first_send_rules()
fresh_operation = fresh.__anext__()
try:
    fresh_operation.send(9)
except BaseException as exc:
    print("first-send", type(exc).__name__, str(exc))
try:
    fresh_operation.send(None)
except BaseException as exc:
    print("first-send-reuse", type(exc).__name__, str(exc))
try:
    fresh.__anext__().send(None)
except StopIteration as exc:
    print("first-send-generator-reusable", exc.value)

async def overlap():
    await Pause()
    yield 3

running = overlap()
owner = running.__anext__()
blocked = running.__anext__()
print("overlap-token", owner.send(None), running.ag_running)
try:
    blocked.send(None)
except RuntimeError as exc:
    print("overlap", str(exc))
owner.close()
print("owner-wrapper-close", running.ag_running)
try:
    blocked.send(None)
except RuntimeError as exc:
    print("blocked-reuse", str(exc))
after_owner_close = running.__anext__()
try:
    after_owner_close.send(None)
except RuntimeError as exc:
    print("owner-close-overlap", str(exc))

async def ignores_close():
    try:
        yield 10
    except GeneratorExit:
        yield 11

ignoring = ignores_close()
try:
    ignoring.__anext__().send(None)
except StopIteration as exc:
    print("ignore-first", exc.value)
try:
    ignoring.aclose().send(None)
except RuntimeError as exc:
    print("ignore-close", str(exc))

async_cleanup_events = []

async def async_cleanup():
    try:
        yield 12
    finally:
        async_cleanup_events.append("before")
        resumed = await Pause()
        async_cleanup_events.append("after-" + str(resumed))

cleanup_generator = async_cleanup()
try:
    cleanup_generator.__anext__().send(None)
except StopIteration as exc:
    print("cleanup-first", exc.value)
cleanup_operation = cleanup_generator.aclose()
print(
    "cleanup-token",
    cleanup_operation.send(None),
    cleanup_generator.ag_running,
    cleanup_generator.ag_await is not None,
)
try:
    cleanup_operation.send(13)
except StopIteration as exc:
    print(
        "cleanup-close",
        exc.value,
        async_cleanup_events,
        cleanup_generator.ag_running,
        cleanup_generator.ag_await,
    )

async def raises_stop_async_iteration():
    raise StopAsyncIteration("bad")
    yield 0

bad = raises_stop_async_iteration()
try:
    bad.__anext__().send(None)
except BaseException as exc:
    print("stop-async", type(exc).__name__, str(exc))

class CancelSignal(BaseException):
    pass

cancel_events = []

async def cancellation_cleanup():
    try:
        await Pause()
        yield "unreachable"
    finally:
        cancel_events.append("cancel-finally")

cancelled = cancellation_cleanup()
cancel_operation = cancelled.__anext__()
print("cancel-token", cancel_operation.send(None))
try:
    cancel_operation.throw(CancelSignal("cancelled"))
except BaseException as exc:
    print("cancelled", type(exc).__name__, str(exc), cancel_events)

async def traceback_generator():
    await Pause()
    raise KeyError("trace")
    yield 0

traced = traceback_generator()
traced_operation = traced.__anext__()
print("trace-token", traced_operation.send(None))
try:
    traced_operation.send(None)
except KeyError as exc:
    names = []
    traceback = exc.__traceback__
    while traceback is not None:
        names.append(traceback.tb_frame.f_code.co_name)
        traceback = traceback.tb_next
    print("traceback", names)

async def long_stream(limit):
    index = 0
    while index < limit:
        yield index
        index += 1

stream = long_stream(5000)
total = 0
while True:
    operation = stream.__anext__()
    try:
        operation.send(None)
    except StopIteration as exc:
        total += exc.value
    except StopAsyncIteration:
        break
print("long", total)
