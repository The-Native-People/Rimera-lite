events = []

class Heavy:
    def __init__(self, name):
        self.name = name

    def __get__(self, instance, owner):
        junk = None
        for ignored in range(80):
            junk = "hhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhhh"
        events.append("get:" + self.name)
        if instance is None:
            return self
        if self.name == "items":
            return instance._items
        return instance._label

class Node:
    __match_args__ = ("items", "label")
    items = Heavy("items")
    label = Heavy("label")

    def __init__(self, items, label):
        self._items = items
        self._label = label

subject = {"node": Node([1, 2, 3, 4], "alive")}
previous = "original"
match subject:
    case {"node": Node([first, *middle, last], previous)}:
        print("matched", first, middle, last, previous)
    case _:
        print("bad")

print(events)
