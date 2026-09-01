class OneShot:
    def __init__(self, values):
        self.values = values
        self.index = 0

    def __iter__(self):
        return self

    def __next__(self):
        if self.index >= len(self.values):
            raise StopIteration
        value = self.values[self.index]
        self.index += 1
        return value


class Holder:
    def __init__(self):
        self.value = None


*head, tail = OneShot([1, 2, 3])
print(head, tail)

left, *middle, right = OneShot([4, 5, 6, 7])
print(left, middle, right)

first, *rest = OneShot([8, 9, 10])
print(first, rest)

outer_left, (*nested_head, nested_tail), outer_right = OneShot(
    [11, OneShot([12, 13, 14]), 15]
)
print(outer_left, nested_head, nested_tail, outer_right)

holder = Holder()
slots = [None]
*holder.value, attr_tail = OneShot([16, 17, 18])
*slots[0], item_tail = OneShot([19, 20, 21])
print(holder.value, attr_tail, slots, item_tail)

preserved = "old"
try:
    preserved, *middle_fail, final_fail = OneShot([22])
except ValueError as error:
    print(type(error).__name__, str(error))
print(preserved)
