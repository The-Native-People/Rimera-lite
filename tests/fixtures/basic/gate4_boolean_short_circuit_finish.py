events = []


class Truth:
    def __init__(self, label, value):
        self.label = label
        self.value = value

    def __bool__(self):
        events.append(("bool", self.label))
        return self.value

    def __repr__(self):
        return "Truth(" + self.label + ")"


class LengthTruth:
    def __init__(self, label, length):
        self.label = label
        self.length = length

    def __len__(self):
        events.append(("len", self.label))
        return self.length

    def __repr__(self):
        return "LengthTruth(" + self.label + ")"


def boom():
    events.append("boom")
    raise RuntimeError("should be skipped")


false_value = Truth("false", False)
true_value = Truth("true", True)
empty = LengthTruth("empty", 0)
full = LengthTruth("full", 2)

result = false_value and boom()
print(result, result is false_value)
result = true_value or boom()
print(result, result is true_value)
result = true_value and false_value and boom()
print(result, result is false_value)
result = false_value or true_value or boom()
print(result, result is true_value)
result = empty or full
print(result, result is full)
result = full and "selected"
print(result)
print(events)


def nested(flag):
    selected = (flag and [1, 2, 3]) or [4, 5]
    if selected and len(selected):
        return selected
    return []


print(nested(True))
print(nested(False))