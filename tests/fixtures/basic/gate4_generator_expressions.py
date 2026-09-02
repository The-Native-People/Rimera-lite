events = []


def mark(label, value):
    events.append(label)
    return value


numbers = [1, 2, 3]
generator = (
    mark("element" + str(item), item * 10)
    for item in mark("outer", numbers)
    if mark("filter" + str(item), item != 2)
)
print(events)
print(next(generator))
print(events)
print(list(generator))
print(events)
print(list(generator))
try:
    print(next(generator))
except StopIteration:
    print("exhausted-again")

nested = (
    left + right
    for left in [1, 2]
    for right in mark("inner" + str(left), [10, 20])
)
print(next(nested))
print(events)
print(list(nested))

def closure_case():
    base = 5
    closure = (base + item for item in [1, 2])
    base = 100
    return list(closure)


print(closure_case())

exceptional = (10 // (item - 2) for item in [1, 2, 3])
print(next(exceptional))
try:
    print(next(exceptional))
except ZeroDivisionError:
    print("generator-error")
print(list(exceptional))
