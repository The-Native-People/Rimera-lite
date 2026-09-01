class Callable:
    def __call__(self, value, *, amount=2):
        return value + amount


class Iterable:
    def __iter__(self):
        return iter([3, 4, 5])


print(Callable()(40, amount=2))
total = 0
for value in Iterable():
    total = total + value
print(total)
