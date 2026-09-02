# Shared-subset runtime benchmark for Rimera and CPython.
#
# Keep this fixture intentionally boring: no imports, stdlib modules,
# comprehensions, generators, async, or other Python features that Rimera
# does not currently claim. Build Rimera first and benchmark the produced
# executable separately so compiler time is never included.

ITERATIONS = 200000


def integer_work(n):
    total = 0
    for i in range(n):
        total += (i * 3 + 7) % 97
    return total


def function_work(n):
    total = 0
    for i in range(n):
        total += mix(i % 101, i % 37)
    return total


def mix(left, right):
    return (left * 3 + right * 5) // 2


def list_work(n):
    values = [1, 3, 5, 7, 11, 13, 17, 19]
    total = 0
    for i in range(n):
        index = i % 8
        values[index] += i % 5
        total += values[index]
    return total + values[0] + values[-1]


def dict_work(n):
    values = {"a": 1, "b": 2, "c": 3, "d": 4}
    total = 0
    for i in range(n):
        selector = i % 4
        if selector == 0:
            values["a"] += 1
            total += values["a"]
        elif selector == 1:
            values["b"] += 2
            total += values["b"]
        elif selector == 2:
            values["c"] += 3
            total += values["c"]
        else:
            values["d"] += 4
            total += values["d"]
    return total


class Counter:
    def __init__(self, value):
        self.value = value

    def step(self, amount):
        self.value += amount
        return self.value


def object_work(n):
    counter = Counter(0)
    total = 0
    for i in range(n):
        total += counter.step((i % 7) + 1)
    return total + counter.value


integer_result = integer_work(ITERATIONS)
function_result = function_work(ITERATIONS)
list_result = list_work(ITERATIONS)
dict_result = dict_work(ITERATIONS)
object_result = object_work(ITERATIONS // 4)

print("iterations", ITERATIONS)
print("integer", integer_result)
print("function", function_result)
print("list", list_result)
print("dict", dict_result)
print("object", object_result)
print("checksum", integer_result + function_result + list_result + dict_result + object_result)
