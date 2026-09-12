from rimera import async_runtime

ITERATIONS = 2500000


async def leaf(value):
    return value + 1


async def main():
    index = 0
    total = 0
    while index < ITERATIONS:
        total += await leaf(index)
        index += 1
    return total


print(async_runtime.run(main()))
