class First:
    value = "first"

class Second:
    value = "second"

class Child(First):
    pass

class Grandchild(Child):
    pass

print(Child().value)
Child.__bases__ = (Second,)
print(Child().value)
print(Grandchild().value)
