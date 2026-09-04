def make(seed):
    holder = []

    def function(value=holder, *, flag=seed):
        return value[0] is function, flag

    holder.append(function)
    function.__annotations__["self"] = function
    return function


class Box:
    def method(self):
        return self


def make_box():
    box = Box()
    box.bound = box.method
    return box


for index in range(120):
    discarded = make(index)
    discarded_box = make_box()
    discarded = None
    discarded_box = None
    pressure = [index, index + 1, index + 2, index + 3]

survivor = make(999)
survivor_box = make_box()
for index in range(120):
    pressure = [index, index + 1, index + 2, index + 3]

print(survivor())
print(survivor.__defaults__[0][0] is survivor)
print(survivor.__annotations__["self"] is survivor)
print(survivor_box.bound() is survivor_box)
