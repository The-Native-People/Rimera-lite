class Counter:
    def __init__(self, limit):
        self.limit = limit
        self.index = 0

    def __aiter__(self):
        return self

    async def __anext__(self):
        if self.index >= self.limit:
            raise StopAsyncIteration
        value = self.index
        self.index += 1
        return value

async def run():
    values = [value * 2 async for value in Counter(5) if value % 2 == 0]
    unique = {value async for value in Counter(4) if value > 1}
    mapping = {value: value + 10 async for value in Counter(3)}
    mixed = [(left, right) for left in [1, 2] async for right in Counter(2)]
    nested = [
        (left, right)
        async for left in Counter(2)
        for right in [10, 20]
        if left + right > 10
    ]
    expression = (value * 3 async for value in Counter(4) if value != 1)
    generated = []
    while True:
        operation = expression.__anext__()
        try:
            operation.send(None)
        except StopIteration as exc:
            generated.append(exc.value)
        except StopAsyncIteration:
            break
    return values, unique, mapping, mixed, nested, generated

value = 999
coroutine = run()
try:
    coroutine.send(None)
except StopIteration as exc:
    values, unique, mapping, mixed, nested, generated = exc.value
    print(values)
    print(sorted(unique))
    print(mapping)
    print(mixed)
    print(nested)
    print(generated)
print(value)
