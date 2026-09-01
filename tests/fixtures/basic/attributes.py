class Counter:
    label = "counter"

    def add(self, amount):
        self.value = self.value + amount
        return self.value

    def describe(self):
        return self.label

    def count(self, remaining):
        if remaining == 0:
            return 0
        return self.count(remaining - 1) + 1

first = Counter()
second = Counter()
first.value = 1
second.value = 10

saved = first.add
print(first.label)
print(saved(2))
print(first.value)
print(second.value)
print(first.describe())
print(first.count(4))

Counter.label = "updated"
print(first.label)
print(Counter.label)

del first.value
try:
    print(first.value)
except AttributeError:
    print("instance missing")

del Counter.label
try:
    print(Counter.label)
except AttributeError:
    print("class missing")

Dynamic = type("Dynamic", (), {})
Dynamic.note = "dynamic"
print(Dynamic.note)
