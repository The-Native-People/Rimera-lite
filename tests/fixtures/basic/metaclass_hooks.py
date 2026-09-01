class Meta(type):
    def __new__(mcls, name, bases, namespace):
        print("new", name)
        return type(name, bases, namespace)

    def __init__(cls, name, bases, namespace):
        print("should not initialize", name)


class Created(metaclass=Meta):
    pass


print(type(Created) == Meta)


class InitMeta(type):
    @classmethod
    def __prepare__(mcls, name, bases):
        print("prepare", name)
        return {}

    def __init__(cls, name, bases, namespace):
        print("init", name)


class Initialized(metaclass=InitMeta):
    pass


print(type(Initialized) == InitMeta)
