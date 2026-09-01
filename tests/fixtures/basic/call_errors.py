def f(a, /, b, *, c):
    return a + b + c


try:
    f()
except TypeError as error:
    print(error)

try:
    f(a=1, b=2, c=3)
except TypeError as error:
    print(error)

try:
    f(1, 2, c=3, b=4)
except TypeError as error:
    print(error)

try:
    f(1, 2, 3, c=4)
except TypeError as error:
    print(error)

try:
    f(1, 2, c=3, d=4)
except TypeError as error:
    print(error)
