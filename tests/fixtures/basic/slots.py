class Pair:
    __slots__ = ("left", "right")

    def total(self):
        return self.left + self.right


class NamedPair(Pair):
    __slots__ = "name"


pair = NamedPair()
pair.left = 20
pair.right = 22
pair.name = "answer"
print(pair.total())
print(pair.name)
del pair.name
print(pair.left)
