events = []


def pack(prefix, *values, tail):
    return prefix, values, tail


class Scale:
    def __init__(self, factor):
        self._factor = factor

    @property
    def factor(self):
        events.append("factor")
        return self._factor

    def stream(self, bias=1):
        factor = self.factor
        values = [factor + item for item in range(3)]
        match values:
            case [first, *middle, last]:
                events.append(("match", first, middle, last))
        try:
            delegated = (value + bias for value in values)
            result = yield from delegated
            events.append(("delegate-result", result))
            received = yield pack("payload", *values, tail=bias)
            events.append(("received", received))
        except ValueError as error:
            events.append(("caught", str(error)))
            yield "handled"
        finally:
            events.append("finally")
        return factor + bias


normal = Scale(3).stream()
print(next(normal))
print(next(normal))
print(next(normal))
print(next(normal))
try:
    normal.send(9)
except StopIteration as error:
    print(error.value)
print(events)

events.clear()
thrown = Scale(5).stream(2)
print(next(thrown))
print(thrown.throw(ValueError("boom")))
try:
    next(thrown)
except StopIteration as error:
    print(error.value)
print(events)
