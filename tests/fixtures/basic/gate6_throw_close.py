events = []


def catcher():
    try:
        yield "ready"
    except ValueError as error:
        events.append(("caught", str(error), error.__traceback__.tb_lineno))
        yielded = yield "handled"
        events.append(("after-send", yielded))
    finally:
        events.append("finally")
    return 7


generator = catcher()
print(next(generator))
print(generator.throw(ValueError("boom")))
print(events)
try:
    generator.send(12)
except StopIteration as error:
    print(error.args, error.value)
print(events)


def closer():
    try:
        yield "open"
    finally:
        events.append("closed")


closed = closer()
print(next(closed))
print(closed.close())
print(events)


def ignores_close():
    try:
        yield "open"
    except GeneratorExit:
        yield "ignored"


ignored = ignores_close()
print(next(ignored))
try:
    ignored.close()
except RuntimeError as error:
    print(type(error).__name__, str(error))


def pep479():
    yield "start"
    raise StopIteration("inner")


converted = pep479()
print(next(converted))
try:
    next(converted)
except RuntimeError as error:
    print(type(error).__name__, str(error))
    print(type(error.__cause__).__name__, str(error.__cause__))
    print(type(error.__context__).__name__, str(error.__context__))

fresh = catcher()
try:
    fresh.throw(ValueError("before"))
except ValueError as error:
    print(type(error).__name__, str(error), events)
