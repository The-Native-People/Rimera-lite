big = 2 ** 100
big_float = float(big)
near = 2 ** 53 + 1
near_float = float(near)

print(big == big_float, hash(big) == hash(big_float))
print(near == near_float, hash(near) == hash(near_float))
print(near > near_float, near_float < near)
nan_value = float("nan")
print(nan_value == nan_value, nan_value != nan_value, nan_value < 0, hash(nan_value) == hash(nan_value))
print(hash(0.5), hash(1.5), hash(float("inf")), hash(float("-inf")), hash(-0.0))
print(complex(big_float, 0) == big, hash(complex(big_float, 0)) == hash(big))
print(len({big: 1, big_float: 2}), {big: 1, big_float: 2}[big])
print(len({near: 1, near_float: 2}))
print(len({True: "a", 1.0: "b", complex(1, 0): "c"}))

class HugeHash:
    def __hash__(self):
        return 2 ** 100

class MinusOneHash:
    def __hash__(self):
        return -1

print(hash(HugeHash()) == hash(2 ** 100), hash(MinusOneHash()))

class EqualKey:
    def __init__(self, label):
        self.label = label

    def __hash__(self):
        return 11

    def __eq__(self, other):
        return self.label == other.label

first = EqualKey("x")
second = EqualKey("x")
equal_dict = {first: 1}
equal_dict[second] = 2
print(len(equal_dict), next(iter(equal_dict)) is first, equal_dict[first])

class ClearingKey:
    def __init__(self, container, clear):
        self.container = container
        self.clear = clear

    def __hash__(self):
        return 7

    def __eq__(self, other):
        if self.clear:
            self.container.clear()
        return True

mutating_dict = {}
old_key = ClearingKey(mutating_dict, True)
mutating_dict[old_key] = 1
new_key = ClearingKey(mutating_dict, False)
mutating_dict[new_key] = 2
print(len(mutating_dict), next(iter(mutating_dict)) is new_key, mutating_dict[new_key])

membership_dict = {}
old_key = ClearingKey(membership_dict, True)
membership_dict[old_key] = 1
new_key = ClearingKey(membership_dict, False)
print(new_key in membership_dict, len(membership_dict))

membership_set = set()
old_key = ClearingKey(membership_set, True)
membership_set.add(old_key)
new_key = ClearingKey(membership_set, False)
print(new_key in membership_set, len(membership_set))

add_set = set()
old_key = ClearingKey(add_set, True)
add_set.add(old_key)
new_key = ClearingKey(add_set, False)
add_set.add(new_key)
print(len(add_set))

remove_set = set()
old_key = ClearingKey(remove_set, True)
remove_set.add(old_key)
new_key = ClearingKey(remove_set, False)
try:
    remove_set.remove(new_key)
except KeyError:
    print("remove-keyerror", len(remove_set))

pop_dict = {}
old_key = ClearingKey(pop_dict, True)
pop_dict[old_key] = 1
new_key = ClearingKey(pop_dict, False)
try:
    pop_dict.pop(new_key)
except KeyError:
    print("pop-keyerror", len(pop_dict))

class HashClearingKey:
    def __init__(self, container, clear):
        self.container = container
        self.clear = clear

    def __hash__(self):
        if self.clear:
            self.container.clear()
        return 13

    def __eq__(self, other):
        return True

hash_dict = {}
old_key = HashClearingKey(hash_dict, False)
hash_dict[old_key] = 1
new_key = HashClearingKey(hash_dict, True)
print(new_key in hash_dict, len(hash_dict))

class ExplodingKey:
    def __hash__(self):
        return 17

    def __eq__(self, other):
        raise ValueError("boom")

exploding_dict = {ExplodingKey(): 1}
try:
    exploding_dict[ExplodingKey()]
except ValueError:
    print("eq-error")

class CountingHash:
    def __init__(self, label):
        self.label = label
        self.calls = 0

    def __hash__(self):
        self.calls += 1
        return 19

    def __eq__(self, other):
        return self.label == other.label

left_key = CountingHash("left")
right_key = CountingHash("right")
count_left = {left_key: 1}
count_right = {right_key: 2}
count_merged = count_left | count_right
print(left_key.calls, right_key.calls, len(count_merged))
count_left |= count_right
print(left_key.calls, right_key.calls, len(count_left))

left = {"a": 1, "x": 1}
right = {"b": 2, "x": 3}
merged = left | right
print(list(merged.items()), list(left.items()), list(right.items()))

identity = id(left)
left |= [("c", 4), ("x", 5)]
print(id(left) == identity, list(left.items()))

class Mapping:
    def keys(self):
        return ["d"]

    def __getitem__(self, key):
        return 6

left |= Mapping()
print(list(left.items()))
try:
    {} | Mapping()
except TypeError:
    print("dict-or-type")
