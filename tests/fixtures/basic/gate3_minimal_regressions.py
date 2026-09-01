values = {1, 2, 3}
values.intersection_update({2, 3, 4})
print(sorted(values))

values = {1, 2, 3}
values.difference_update({2, 4})
print(sorted(values))

values = {1, 2, 3}
values.symmetric_difference_update({3, 4})
print(sorted(values))

items = [0, 3]
items[1:1] = [1, 2]
print(items)

buffer = bytearray([0, 3])
buffer[1:1] = bytes([1, 2])
print(list(buffer))

items = [0, 1, 2, 3, 4, 5]
del items[5:0:-2]
print(items)

buffer = bytearray([0, 1, 2, 3, 4, 5])
del buffer[5:0:-2]
print(list(buffer))
