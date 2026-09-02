events = []

class OneShot:
    def __init__(self, values):
        self.values = values
        self.index = 0

    def __repr__(self):
        return "OneShot"

    def __iter__(self):
        events.append("iter")
        return self

    def __next__(self):
        if self.index >= len(self.values):
            raise StopIteration
        value = self.values[self.index]
        self.index += 1
        events.append(("next", value))
        return value

class Sample:
    left, (middle, *tail) = OneShot([1, OneShot([2, 3, 4])])
    first = second = 9

print(Sample.left, Sample.middle, Sample.tail, Sample.first, Sample.second)
print(events)
