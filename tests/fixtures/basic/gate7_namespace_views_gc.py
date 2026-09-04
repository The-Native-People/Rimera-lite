def make_snapshot(seed):
    payload = [seed]
    snapshot = locals()
    snapshot["cycle"] = snapshot
    return snapshot


for index in range(160):
    dead = make_snapshot(index)
    dead = None
    pressure = [index, index + 1, index + 2, index + 3]

survivor = make_snapshot(999)
for index in range(160):
    pressure = [index, index + 1, index + 2, index + 3]

print(survivor["seed"], survivor["payload"][0], survivor["cycle"] is survivor)


def suspended(seed):
    payload = [seed]
    snapshot = locals()
    snapshot["cycle"] = snapshot
    yield snapshot
    payload.append(seed + 1)
    yield locals()


generator = suspended(7)
first = next(generator)
for index in range(160):
    pressure = [index, index + 1, index + 2, index + 3]
second = next(generator)
print(first is second, second["payload"], second["cycle"] is second)
