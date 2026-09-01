class Parent:
    def name(self):
        return "parent"


class Child(Parent):
    def name(self):
        return super().name() + " child"


print(Child().name())
