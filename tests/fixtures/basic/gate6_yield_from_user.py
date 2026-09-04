events = []


class Delegate:
    def __init__(self):
        self.step = 0

    def __iter__(self):
        events.append("iter")
        return self

    def __next__(self):
        self.step += 1
        events.append(("next", self.step))
        if self.step == 1:
            return 10
        raise StopIteration(40)

    def send(self, value):
        events.append(("send", value))
        return 20

    def throw(self, error):
        events.append(("throw", type(error).__name__, str(error)))
        raise StopIteration(30)

    def close(self):
        events.append("close")


def via_next():
    result = yield from Delegate()
    return result + 1


generator = via_next()
print(next(generator))
try:
    next(generator)
except StopIteration as error:
    print(error.value)
print(events)

events.clear()
generator = via_next()
print(next(generator))
print(generator.send(7))
try:
    generator.throw(ValueError("boom"))
except StopIteration as error:
    print(error.value)
print(events)

events.clear()
generator = via_next()
print(next(generator))
print(generator.close())
print(events)
