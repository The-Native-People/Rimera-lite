class Empty:
    pass

first = Empty()
second = Empty()
Dynamic = type("Dynamic", (), {})
dynamic = Dynamic()

print(type(Empty))
print(type(first))
print(type(first) == Empty)
print(type(second) == Empty)
print(isinstance(first, Empty))
print(isinstance(first, (int, (Empty, str))))
print(isinstance(first, object))
print(issubclass(Empty, object))
print(type(Dynamic))
print(type(dynamic))
print(isinstance(dynamic, Dynamic))
print(issubclass(Dynamic, object))
