try:
    from rimera import async_runtime
except ModuleNotFoundError:
    class Runtime:
        def run(self, coroutine):
            try:
                coroutine.send(None)
            except StopIteration as result:
                return result.value
            raise RuntimeError("unexpected suspension")
    async_runtime = Runtime()


async def pure():
    return 1


count = 0


async def impure():
    global count
    count += 1
    return count


pure = impure


async def main():
    value = 0
    for index in range(1000):
        value = await pure()
    return value


print(async_runtime.run(main()))
print(count)
