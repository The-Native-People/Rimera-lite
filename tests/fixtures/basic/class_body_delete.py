class Temporary:
    value = 41
    del value
    replacement = 42


print(Temporary.replacement)
try:
    print(Temporary.value)
except AttributeError:
    print("deleted")
