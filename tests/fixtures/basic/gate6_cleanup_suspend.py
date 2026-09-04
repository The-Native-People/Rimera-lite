events = []


def pending_finally():
    try:
        raise ValueError("boom")
    finally:
        events.append("before-yield")
        received = yield "pause"
        events.append(("after-yield", received))


generator = pending_finally()
print(next(generator))
print(events)
try:
    generator.send(5)
except ValueError as error:
    print(type(error).__name__, str(error))
print(events)


def caught_then_finally():
    try:
        try:
            raise ValueError("key")
        except ValueError as error:
            events.append(("caught", str(error)))
            yield "caught-yield"
        finally:
            events.append("inner-finally")
            yield "finally-yield"
    finally:
        events.append("outer-finally")


generator = caught_then_finally()
print(next(generator))
print(next(generator))
print(generator.close())
print(events)


def return_through_finally():
    try:
        return 11
    finally:
        events.append("return-finally")
        yield "return-pause"


generator = return_through_finally()
print(next(generator))
try:
    next(generator)
except StopIteration as error:
    print(error.args, error.value)
print(events)
