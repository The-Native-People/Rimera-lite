class Box:
    value = 4


class CallableBox:
    def __call__(self):
        return 1


box = Box()
print(getattr(box, "value"))
print(getattr(box, "missing", 9))
setattr(box, "value", 7)
print(getattr(box, "value"))
print(hasattr(box, "value"))
delattr(box, "value")
print(hasattr(box, "value"))
print(callable(CallableBox()))
print(callable(box))
