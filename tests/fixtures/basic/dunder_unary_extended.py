class Unary:
    def __pos__(self):
        return 40

    def __invert__(self):
        return 41


print(+3)
print(~3)
print(+Unary())
print(~Unary())
