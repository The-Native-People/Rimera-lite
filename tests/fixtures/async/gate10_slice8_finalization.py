events = []

async def abandoned_cycle(holder):
    try:
        yield 1
    finally:
        events.append("cycle-finally")

holder = []
generator = abandoned_cycle(holder)
holder.append(generator)
operation = generator.__anext__()
try:
    operation.send(None)
except StopIteration as exc:
    print("started", exc.value)

del operation
del generator
del holder

# Retained allocations deterministically cross both CPython's cyclic-GC
# scheduling threshold and Rimera's managed-heap collection threshold.
junk = []
index = 0
while index < 2500:
    junk.append([index, index + 1, index + 2])
    index += 1

print(events)

async def shutdown_generator():
    try:
        yield 2
    finally:
        print("shutdown-finally")

shutdown = shutdown_generator()
try:
    shutdown.__anext__().send(None)
except StopIteration as exc:
    print("shutdown-started", exc.value)
print("before-shutdown")
