obj = []
print((alias := obj) is obj)

if (condition := 3):
    print(condition)

counter = 0
while (value := counter) < 2:
    print(value)
    counter += 1


def outer():
    x = 1

    def inner():
        nonlocal x
        return (x := x + 4)

    return inner(), x


print(outer())
g = 1


def bump():
    global g
    return (g := g + 2)


print(bump(), g)
make = lambda value: (captured := value + 1)
print(make(4))


class Holder:
    (member := 12)


print(Holder.member)
