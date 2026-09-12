from rimera import async_runtime


async def leaf():
    return 41


async def main():
    child = leaf()
    try:
        async_runtime.run(child)
    except RuntimeError as exc:
        print(str(exc))
    finally:
        child.close()
    return await leaf()


print("outer", async_runtime.run(main()))


async def failing():
    raise ValueError("root-failure")


try:
    async_runtime.run(failing())
except ValueError as exc:
    print(str(exc))
print("next", async_runtime.run(leaf()))
