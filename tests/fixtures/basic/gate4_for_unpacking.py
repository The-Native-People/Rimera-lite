class RowIter:
    def __init__(self):
        self.index = 0

    def __iter__(self):
        return self

    def __next__(self):
        rows = [[1, 2], [3, 4, 5], [6, [7, 8]]]
        if self.index == 3:
            raise StopIteration
        value = rows[self.index]
        self.index += 1
        return value


pairs = []
for left, right in [[1, 2], [3, 4]]:
    pairs.append((left, right))
else:
    pairs.append(("else", 5))
print(pairs)

stars = []
for first, *middle, last in [[1, 2, 3, 4], [5, 6]]:
    stars.append((first, middle, last))
print(stars)

nested = []
for left, (middle, right) in [[1, [2, 3]], [4, [5, 6]]]:
    nested.append((left, middle, right))
print(nested)

log = []
try:
    for left, right in [[10, 11], [12]]:
        log.append(("body", left, right))
    else:
        log.append("else")
except ValueError as error:
    log.append(("error", str(error)))
finally:
    log.append("finally")
print(log)

control = []
for left, right in [[1, 2], [3, 4], [5, 6]]:
    if left == 1:
        continue
    control.append((left, right))
    if left == 3:
        break
else:
    control.append("else")
print(control)

class Body:
    total = 0
    for left, *middle, right in [[1, 2, 3], [4, 5, 6, 7]]:
        total += left + right + len(middle)
    else:
        total += 100


print(Body.total)
