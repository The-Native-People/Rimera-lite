class Box:
    pass


class Holder:
    values = [1]
    values[0] = 2
    item = Box()
    item.value = 3
    del item.value


print(Holder.values[0])
try:
    print(Holder.item.value)
except AttributeError:
    print("attribute deleted")
