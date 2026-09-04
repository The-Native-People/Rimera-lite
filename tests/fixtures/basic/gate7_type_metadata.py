class Base:
    marker = 7


class Outer:
    class Inner(Base):
        pass


Child = Outer.Inner
print(Child.__name__)
print(Child.__qualname__)
print(Child.__module__)
print(tuple(base.__name__ for base in Child.__bases__))
print(tuple(base.__name__ for base in Child.__mro__))
print(Base.__dict__["marker"], Child.marker)

Child.__name__ = "Renamed"
Child.__qualname__ = "Outer.Renamed"
Child.__module__ = 123
print(Child.__name__)
print(Child.__qualname__)
print(Child.__module__)

try:
    Child.__mro__ = ()
except AttributeError:
    print("mro readonly")

try:
    del Child.__module__
except TypeError:
    print("module delete blocked")

for text in ("0x1.8p+1", "-0x0.8p+0", "1", "  +0X1.Fp2  ", "inf"):
    print(float.fromhex(text).hex())


class F(float):
    pass


value = F.fromhex("0x1p+2")
print(type(value).__name__, value)
