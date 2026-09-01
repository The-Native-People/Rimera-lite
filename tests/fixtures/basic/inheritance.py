class Root:
    shared = "root"

    def speak(self):
        return "root"


class Left(Root):
    def speak(self):
        return "left"

    def next_side(self):
        return super(Left, self).side()


class Right(Root):
    def side(self):
        return "right"


class Diamond(Left, Right):
    def speak(self):
        return super(Diamond, self).speak() + "-diamond"


value = Diamond()
print(value.speak())
print(value.next_side())
print(value.shared)
print(isinstance(value, Diamond))
print(isinstance(value, Root))
print(issubclass(Diamond, Left))
print(issubclass(Diamond, Right))
print(issubclass(Diamond, Root))

Root.shared = "changed"
print(value.shared)
Diamond.shared = "diamond"
print(value.shared)
del Diamond.shared
print(value.shared)

Dynamic = type("Dynamic", (Diamond,), {})
dynamic = Dynamic()
print(dynamic.speak())
print(isinstance(dynamic, Root))
print(type(dynamic) == Dynamic)
