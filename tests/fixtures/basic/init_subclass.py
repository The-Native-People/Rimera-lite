class Parent:
    def __init_subclass__(cls):
        cls.kind = "child"


class Child(Parent):
    pass


print(Child.kind)
