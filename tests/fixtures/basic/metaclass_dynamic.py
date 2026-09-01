class Meta(type):
    pass


Value = Meta("Value", (), {})
class SourceValue(metaclass=Meta):
    pass
print(type(Meta) == type)
print(type(Value) == Meta)
print(issubclass(Value, object))
print(type(SourceValue) == Meta)
