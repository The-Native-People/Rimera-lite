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


count = 0


async def immediate():
    global count
    count += 1
    return count


async def main():
    value = 0
    for index in range(1000):
        value = await immediate()
    return value


print(async_runtime.run(main()))
print(count)
