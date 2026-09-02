events = []

match [1, 2, 3, 4]:
    case [first, *middle, last]:
        print("star", first, middle, last)

match (5, 6):
    case [left, right]:
        print("tuple", left, right)

match range(3):
    case [zero, one, two]:
        print("range", zero, one, two)

match [1, [2, 3], 4]:
    case [1, [nested, 3], tail]:
        print("nested", nested, tail)

match "abc":
    case [_, _, _]:
        print("bad-string")
    case _:
        print("string-excluded")

match b"ab":
    case [_, _]:
        print("bad-bytes")
    case _:
        print("bytes-excluded")

kept = "old"
match [1, [2, 99]]:
    case [kept, [inner, 3]]:
        print("bad-rollback")
    case _:
        print("rollback", kept)

mapping = {"a": [7, 8], "b": 9, "c": 10}
match mapping:
    case {"a": [seven, eight], "b": nine, **rest}:
        print("mapping", seven, eight, nine, rest)

match {"a": 1}:
    case {"missing": missing}:
        print("bad-missing")
    case _:
        print("missing")

class Probe(dict):
    def __len__(self):
        events.append("len")
        return 2

    def get(self, key, default=None):
        events.append("get:" + key)
        if key == "a":
            return 11
        if key == "b":
            return 12
        return default

probe = Probe()
match probe:
    case {"a": eleven, "b": twelve}:
        print("probe", eleven, twelve)
print(events)

class DuplicateKeys:
    first = "same"
    second = "same"

try:
    match {"same": 1, "other": 2}:
        case {DuplicateKeys.first: duplicate_left, DuplicateKeys.second: duplicate_right}:
            print("bad-duplicate")
except ValueError:
    print("duplicate-key")
