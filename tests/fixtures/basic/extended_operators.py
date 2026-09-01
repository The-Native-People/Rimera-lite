print(7 / 2)
print(2 ** 10)
print(3 << 4)
print(128 >> 3)
print(6 & 3, 6 ^ 3, 6 | 3)


class Reflected:
    def __rpow__(self, value):
        return value + 40


print(2 ** Reflected())
