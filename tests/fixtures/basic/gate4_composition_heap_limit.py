mapping = {"stable": [1, 2], "other": 3}

first = "first-old"
second = "second-old"
third = "third-old"
try:
    first, (second, third) = [10, [20]]
except ValueError:
    unpack_failed = True

pattern = "pattern-old"
match [1, [2, 99]]:
    case [pattern, [inner, 3]]:
        print("bad-pattern")
    case _:
        pattern_failed = True

print("before", mapping, first, second, third, pattern)

try:
    pressure = [value for value in range(20000)]
    print("bad-pressure", len(pressure))
except MemoryError:
    print("memory", mapping, first, second, third, pattern)
    print("after", len(mapping), mapping["stable"][1])
