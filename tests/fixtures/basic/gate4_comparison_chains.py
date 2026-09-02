events = []


class Result:
    def __init__(self, label, truth):
        self.label = label
        self.truth = truth

    def __bool__(self):
        events.append(("bool", self.label))
        return self.truth

    def __repr__(self):
        return "Result(" + self.label + ")"


class Box:
    def __init__(self, label, value):
        self.label = label
        self.value = value

    def __lt__(self, other):
        label = self.label + "<" + other.label
        events.append(("lt", self.label, other.label))
        return Result(label, self.value < other.value)

    def __eq__(self, other):
        label = self.label + "==" + other.label
        events.append(("eq", self.label, other.label))
        return Result(label, self.value == other.value)

    def __repr__(self):
        return "Box(" + self.label + ")"


def middle(label, value):
    events.append(("middle", label))
    return Box(label, value)


def final(label, value):
    events.append(("final", label))
    return Box(label, value)


left = Box("left", 1)
print(left < middle("middle", 2) < final("final", 3))
print(events)
events.clear()

left = Box("left", 4)
print(left < middle("middle", 2) < final("skipped", 3))
print(events)
events.clear()

same = Box("same", 5)
container = [same]
print(same is same in container)
print(same is not Box("other", 5) != Box("third", 6))

print(1 < 2 == 2 <= 3 > 1 >= 1)
print(3 < 2 < (1 // 0))


class Explode:
    def __lt__(self, other):
        events.append("explode")
        raise ValueError("comparison exploded")


try:
    print(Explode() < Box("x", 1) < Box("y", 2))
except ValueError as error:
    print(type(error).__name__, str(error), events)
