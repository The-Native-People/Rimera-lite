rhs_calls = 0
receiver_calls = 0
index_calls = 0
items = [0]


def rhs():
    global rhs_calls
    rhs_calls += 1
    return 7


def receiver():
    global receiver_calls
    receiver_calls += 1
    return items


def index():
    global index_calls
    index_calls += 1
    return 0


left = right = rhs()
receiver()[index()] = rhs()
head, tail = [1, 2]
first, *rest = [3, 4, 5]
print(left, right, rhs_calls)
print(items, receiver_calls, index_calls)
print(head, tail, first, rest)
