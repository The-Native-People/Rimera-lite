from rimera import async_runtime


async def child(value):
    return value + 1


async def main():
    first = await child(40)
    second = await child(first)
    print("async-root", second)
    return second


print("root-return", async_runtime.run(main()))
