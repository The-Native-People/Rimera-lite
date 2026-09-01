class Protocols:
    def __len__(self):
        return 3

    def __getitem__(self, index):
        return 40 + index

    def __contains__(self, value):
        return value == 7

    def __bool__(self):
        return False


value = Protocols()
print(len(value))
print(value[2])
print(7 in value)
print(8 in value)
if value:
    print("true")
else:
    print("false")
