values = {item % 3 for item in [1, 2, 3, 4, 5, 6]}
print(len(values), 0 in values, 1 in values, 2 in values)

replacement = {item % 2: item for item in [1, 2, 3, 4, 5]}
print(list(replacement.items()))

order = []


def key(value):
    order.append("key" + str(value))
    return value % 2


def mapped(value):
    order.append("value" + str(value))
    return value * 10


result = {key(item): mapped(item) for item in [1, 2, 3]}
print(list(result.items()))
print(order)


class Key:
    def __init__(self, label, group):
        self.label = label
        self.group = group

    def __hash__(self):
        return 17

    def __eq__(self, other):
        return self.group == other.group


first = Key("first", 1)
second = Key("second", 1)
third = Key("third", 2)
custom = {key_object: value for key_object, value in [(first, 10), (second, 20), (third, 30)]}
print(len(custom))
print([(key_object.label, value) for key_object, value in custom.items()])

custom_set = {key_object for key_object in [first, second, third]}
print(len(custom_set), first in custom_set, second in custom_set, third in custom_set)


class BadHash:
    def __hash__(self):
        raise ValueError("hash boom")


try:
    {value for value in [BadHash()]}
except ValueError:
    print("set-hash-error")

try:
    {value: 1 for value in [BadHash()]}
except ValueError:
    print("dict-hash-error")
