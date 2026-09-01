class PreparedNamespace:
    def __init__(self):
        self.values = {}

    def __setitem__(self, key, value):
        self.values[key] = value

    def __getitem__(self, key):
        return self.values[key]

    def __delitem__(self, key):
        self.values = {}

    def keys(self):
        return self.values


class MappingMeta(type):
    @classmethod
    def __prepare__(mcls, name, bases):
        return PreparedNamespace()

    def __new__(mcls, name, bases, namespace):
        return type(name, bases, namespace.values)


class Prepared(metaclass=MappingMeta):
    removed = 1
    del removed
    answer = 42

    def method(self):
        return self.answer


print(Prepared.answer)
print(Prepared().method())
