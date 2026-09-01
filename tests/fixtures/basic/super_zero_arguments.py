class Parent:
    def value(self):
        return "parent"


class Child(Parent):
    def value(self):
        return super().value()
