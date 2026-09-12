try:
    from rimera import async_runtime
except ModuleNotFoundError:
    class _FallbackRuntime:
        def run(self, coroutine):
            try:
                yielded = coroutine.send(None)
            except StopIteration as stop:
                return stop.value
            raise RuntimeError("ready-only oracle unexpectedly suspended: " + str(yielded))

    async_runtime = _FallbackRuntime()

from helper import AsyncRange, Box, Manager, ReadyAwait, apply, chain, leaf, stream, sync_values


events = []


def make_callback(offset):
    def callback(value):
        return value + offset
    return callback


async def scenario():
    total = Box(4).marker
    total += await leaf(10)
    total += await ReadyAwait(5)

    callback = make_callback(2)
    for value in sync_values(5):
        total += apply(callback, value)

    values = [value async for value in AsyncRange(8) if value % 2 == 0]
    for value in values:
        total += value

    async for value in stream(6):
        total += value

    async with Manager(events, "outer") as name:
        events.append("body:" + name)
        async with Manager(events, "inner"):
            total += await chain(80, 1)

    try:
        async with Manager(events, "error"):
            raise ValueError("boom")
    except ValueError as exc:
        events.append("caught:" + str(exc))

    # Keep many coroutine frames live across managed allocation/collection, then
    # resume all of them. This exercises shadow roots without task-per-await.
    pending = []
    index = 0
    while index < 64:
        pending.append(leaf(index))
        index += 1

    # Produce unreachable cycles repeatedly. Under Rimera's constrained-heap
    # run, failure to collect these graphs becomes a deterministic OOM.
    index = 0
    while index < 2500:
        cycle = []
        cycle.append(cycle)
        cycle = None
        [index, index + 1, index + 2]
        index += 1

    for coroutine in pending:
        total += await coroutine

    return total


result = async_runtime.run(scenario())
print("result", result)
print("events", events)


class Pause:
    def __await__(self):
        yield "pause"
        return 1


class Cancelled(BaseException):
    pass


async def suspended_worker(index):
    retained = [index]
    retained.append(retained)
    try:
        async with Manager(events, "worker" + str(index)):
            await Pause()
            return retained[0]
    except Cancelled:
        return -1
    finally:
        events.append("finished:" + str(index))


# Every frame below is actually suspended, retaining a self-cycle and a
# captured manager exit while unrelated allocations force collections.
events.clear()
workers = [suspended_worker(index) for index in range(32)]
for worker in workers:
    assert worker.send(None) == "pause"
for index in range(3000):
    cycle = []
    cycle.append(cycle)
    cycle = None
total = 0
for index in range(32):
    try:
        if index % 2 == 0:
            workers[index].throw(Cancelled())
        else:
            workers[index].send(None)
    except StopIteration as result:
        total += result.value
workers.clear()
print("suspended-workers", total, len(events))
