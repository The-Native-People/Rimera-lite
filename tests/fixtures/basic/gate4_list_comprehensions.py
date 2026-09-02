print([item * 2 for item in [1, 2, 3, 4] if item % 2 == 0])
print([left * right for left in [1, 2] for right in [3, 4] if left + right > 4])
print([first + last for first, *middle, last in [[1, 2, 3], [4, 5, 6, 7]]])


events = []


def select(value):
    events.append("filter" + str(value))
    if value == 3:
        raise ValueError("filter boom")
    return value % 2 == 1


try:
    [value for value in [1, 2, 3, 4] if select(value)]
except ValueError:
    print("filter-error")
print(events)


try:
    [left + right for left, right in [[1, 2], [3]]]
except ValueError:
    print("target-error")


def missing_iterable():
    raise ValueError("iterable boom")


try:
    [value for value in missing_iterable()]
except ValueError:
    print("outer-iterable-error")


def build(depth):
    if depth == 0:
        return [0]
    prior = build(depth - 1)
    return [depth + value for value in prior]


print(build(4))
print([[inner for inner in [outer, outer + 1]] for outer in [1, 3]])
