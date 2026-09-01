class MetaDescriptor:
    def __get__(self, instance, owner):
        return "from metaclass"

    def __set__(self, instance, value):
        print("set", value)


class Meta(type):
    label = MetaDescriptor()


class Value(metaclass=Meta):
    label = "from class"


print(Value.label)
