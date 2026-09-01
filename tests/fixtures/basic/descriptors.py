class Sample:
    label = "class"

    def set_initial(self, value):
        self._value = value

    @property
    def value(self):
        return self._value

    @value.setter
    def value(self, value):
        self._value = value + 1

    @staticmethod
    def add(left, right):
        return left + right

    @classmethod
    def describe(cls):
        return cls.label


sample = Sample()
sample.set_initial(4)
print(sample.value)
sample.value = 9
print(sample.value)
print(Sample.add(2, 3))
print(sample.add(3, 4))
print(Sample.describe())
print(sample.describe())


class Doubled:
    def __get__(self, instance, owner):
        return instance._stored

    def __set__(self, instance, value):
        instance._stored = value * 2


class Holder:
    value = Doubled()


holder = Holder()
holder.value = 6
print(holder.value)
