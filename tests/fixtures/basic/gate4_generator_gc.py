def build_generator():
    prefix = "rooted"
    marker = ["state"]
    generator = (
        prefix + ":" + str(left) + ":" + str(right) + ":" + marker[0]
        for left in [1, 2]
        for right in [10, 20]
    )
    prefix = "mutated"
    marker[0] = "alive"
    return generator


generator = build_generator()
print(next(generator))

junk = None
for ignored in range(200):
    junk = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"

print(next(generator))

for ignored in range(200):
    junk = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"

print(list(generator))
print(list(generator))
