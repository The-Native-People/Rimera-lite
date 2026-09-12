import gc
import json
import warnings


class MarkerError(Exception):
    pass


class TokenAwaitable:
    def __init__(self, label, result=None):
        self.label = label
        self.result = result

    def __await__(self):
        received = yield f"token:{self.label}"
        if received is not None:
            return received
        return self.result


def drive(awaitable, actions=()):
    iterator = awaitable.__await__()
    transcript = []
    actions = iter(actions)
    pending = None
    while True:
        try:
            if isinstance(pending, BaseException):
                yielded = iterator.throw(pending)
            else:
                yielded = iterator.send(pending)
        except StopIteration as stop:
            return {"yielded": transcript, "return": stop.value}
        transcript.append(yielded)
        pending = next(actions, None)


async def leaf(value):
    return await TokenAwaitable("leaf", value + 1)


async def nested():
    return await leaf(40)


async def catches_injected():
    try:
        await TokenAwaitable("inject")
    except MarkerError as error:
        return f"caught:{error}"


async def immediate():
    return 7


async def closable(events):
    try:
        await TokenAwaitable("close")
    finally:
        events.append("finally")


class AsyncCounter:
    def __init__(self, stop):
        self.current = 0
        self.stop = stop

    def __aiter__(self):
        return self

    async def __anext__(self):
        if self.current >= self.stop:
            raise StopAsyncIteration
        value = self.current
        self.current += 1
        return await TokenAwaitable(f"next:{value}", value)


async def consume_counter():
    values = []
    async for value in AsyncCounter(2):
        values.append(value)
    return values


class AsyncManager:
    def __init__(self, events):
        self.events = events

    async def __aenter__(self):
        self.events.append("enter-start")
        value = await TokenAwaitable("enter", "bound")
        self.events.append("enter-done")
        return value

    async def __aexit__(self, exc_type, exc, tb):
        self.events.append(
            ["exit-start", None if exc_type is None else exc_type.__name__]
        )
        await TokenAwaitable("exit")
        self.events.append("exit-done")
        return False


async def use_manager(events):
    async with AsyncManager(events) as value:
        events.append(["body", value])
    return "done"


async def async_generator():
    yield 1
    await TokenAwaitable("agen")
    yield 2


def main():
    report = {}

    report["nested_direct_await"] = drive(nested())
    report["exception_injection"] = drive(
        catches_injected(), [MarkerError("boom")]
    )

    reused = immediate()
    report["first_completion"] = drive(reused)
    try:
        drive(reused)
    except RuntimeError as error:
        report["reuse_error"] = [type(error).__name__, str(error)]

    close_events = []
    close_target = closable(close_events)
    close_iterator = close_target.__await__()
    first = close_iterator.send(None)
    close_target.close()
    report["close"] = {"first": first, "events": close_events}

    report["async_for"] = drive(consume_counter())

    manager_events = []
    report["async_with"] = {
        "drive": drive(use_manager(manager_events)),
        "events": manager_events,
    }

    generator = async_generator()
    first_item = drive(generator.__anext__())
    second_item = drive(generator.__anext__())
    try:
        drive(generator.__anext__())
    except StopAsyncIteration:
        exhausted = "StopAsyncIteration"
    else:
        exhausted = "missing"
    report["async_generator"] = {
        "first": first_item,
        "second": second_item,
        "exhausted": exhausted,
    }

    with warnings.catch_warnings(record=True) as captured:
        warnings.simplefilter("always", RuntimeWarning)
        abandoned = immediate()
        del abandoned
        gc.collect()
    report["unawaited_warning"] = [
        type(item.message).__name__ for item in captured
    ]

    print(json.dumps(report, sort_keys=True, separators=(",", ":")))


if __name__ == "__main__":
    main()
