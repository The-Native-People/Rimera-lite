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


async def immediate():
    return 1


async def main():
    for index in range(250000):
        value = await immediate()
    return value


print(async_runtime.run(main()))
