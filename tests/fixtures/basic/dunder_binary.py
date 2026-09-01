class Direct:
    def __add__(self, other):
        return 40 + other


class Reverse:
    def __radd__(self, other):
        return 42


class Decline:
    def __add__(self, other):
        return NotImplemented


print(Direct() + 2)
print(40 + Reverse())
print(Decline() + Reverse())
print(type(NotImplemented))
