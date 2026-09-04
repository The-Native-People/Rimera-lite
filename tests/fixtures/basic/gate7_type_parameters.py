outer = "outer"


def identity[T, *Ts, **P](value: T) -> T:
    print(type(T).__name__, T.__name__)
    print(type(Ts).__name__, Ts.__name__)
    print(type(P).__name__, P.__name__)
    return value


class Box[T, *Ts, **P]:
    first = T
    rest = Ts
    params = P


type Alias[T] = T

print(tuple(param.__name__ for param in identity.__type_params__))
print(tuple(type(param).__name__ for param in identity.__type_params__))
print(tuple(param.__name__ for param in Box.__type_params__))
print(Box.first is Box.__type_params__[0])
print(Box.rest is Box.__type_params__[1])
print(Box.params is Box.__type_params__[2])
print(Alias.__name__)
print(tuple(param.__name__ for param in Alias.__type_params__))
print(Alias.__value__ is Alias.__type_params__[0])
print(identity(17))
print(outer)
try:
    print(T)
except NameError:
    print("no leak")
