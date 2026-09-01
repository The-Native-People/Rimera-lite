class RootMeta(type):
    pass


class DerivedMeta(RootMeta):
    pass


class Left(metaclass=RootMeta):
    pass


class Right(metaclass=DerivedMeta):
    pass


class Combined(Left, Right):
    pass


print(type(Combined) == DerivedMeta)
