events = []


def source():
    events.append("start")
    received = yield 1
    events.append(("send", received))
    yield received + 1
    return 9


generator = source()
print(events)
print(iter(generator) is generator)
print(next(generator))
print(events)
print(generator.send(4))
print(events)
try:
    next(generator)
except StopIteration as error:
    print(error.args, error.value)
try:
    next(generator)
except StopIteration as error:
    print(error.args, error.value)

fresh = source()
try:
    fresh.send(2)
except TypeError as error:
    print(type(error).__name__, str(error))

never_started = source()
print(never_started.close())
print(events)
