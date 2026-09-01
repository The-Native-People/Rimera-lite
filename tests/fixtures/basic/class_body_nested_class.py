class Outer:
    class Inner:
        value = "inner"

print(Outer.Inner.value)
print(Outer.Inner.__name__)
