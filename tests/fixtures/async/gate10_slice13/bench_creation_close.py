async def immediate():
    return 1


def main():
    for index in range(1000000):
        coroutine = immediate()
        coroutine.close()
    return index


print(main())
