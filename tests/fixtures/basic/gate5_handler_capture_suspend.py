events = []


def catcher():
    try:
        yield "ready"
    except ValueError as error:
        captured = error
        events.append((type(captured).__name__, captured.args[0]))
        yield captured.args[0]
        events.append("resumed")
    finally:
        events.append("finally")


generator = catcher()
print(next(generator))
print(generator.throw(ValueError("boom")))
print(events)
try:
    next(generator)
except StopIteration:
    print("done")
print(events)
