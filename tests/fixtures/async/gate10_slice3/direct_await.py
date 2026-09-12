events = []

async def leaf():
    events.append("leaf")
    return 41

async def main():
    events.append("main")
    value = await leaf()
    events.append("after")
    return value + 1

coroutine = main()
print(type(coroutine).__name__, events)

try:
    coroutine.send(None)
except StopIteration as error:
    print(type(error).__name__, error.args, events)

try:
    coroutine.send(None)
except RuntimeError as error:
    print(type(error).__name__, str(error))
