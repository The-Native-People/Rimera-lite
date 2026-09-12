count = 0


async def original():
    return 1


async def inner():
    return 2


def replacement():
    global count
    count += 1
    return inner()


original = replacement


def main():
    coroutine = None
    for index in range(1000):
        coroutine = original()
        coroutine.close()
    return index, count, coroutine.cr_frame is None


print(main())
