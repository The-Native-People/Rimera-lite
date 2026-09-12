async def leaf(value):
    return value

index = 0
last = -1
while index < 200:
    coro = leaf(index)
    try:
        coro.send(None)
    except StopIteration as exc:
        last = exc.value
    index += 1

print(last)
