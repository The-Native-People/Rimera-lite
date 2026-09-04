events = []


def child():
    events.append("child-start")
    received = yield 1
    events.append(("child-send", received))
    try:
        yield 2
    except ValueError as error:
        events.append(("child-throw", str(error)))
        yield 3
    finally:
        events.append("child-finally")
    return 9


def outer():
    events.append("outer-start")
    result = yield from child()
    events.append(("result", result))
    return result + 1


generator = outer()
print(next(generator))
print(events)
print(generator.send(7))
print(events)
print(generator.throw(ValueError("boom")))
print(events)
try:
    next(generator)
except StopIteration as error:
    print(error.args, error.value)
print(events)


def from_list():
    result = yield from [4, 5]
    print("list-result", result)


generator = from_list()
print(next(generator))
try:
    generator.send(8)
except AttributeError as error:
    print(type(error).__name__)


def nested():
    result = yield from outer()
    return result * 2


generator = nested()
print(next(generator))
print(generator.send(1))
print(generator.throw(ValueError("nested")))
try:
    next(generator)
except StopIteration as error:
    print("nested-result", error.value)


def close_child():
    try:
        yield "open"
    finally:
        events.append("delegated-close")


def close_outer():
    yield from close_child()


generator = close_outer()
print(next(generator))
print(generator.close())
print(events)
