payload = b"abc"
print(payload[1])
print(len(payload))

number = 1 + 2j
print(number)
print(type(number))

window = payload[1:3]
print(window)
print(payload[slice(0, 3, 2)])
print(bytes([65, 66]))
print(bytearray(b"ab"))
print(complex(3))
print(b"a" * 3)
view = memoryview(b"xy")
print(len(view))
print(view[1])
print(view.format, view.itemsize, view.ndim, view.shape, view.strides, view.readonly, view.nbytes)
print(dict(alpha=1))
mapping = dict(alpha=1, beta=2)
print(list(mapping.keys()))
print(list(mapping.values()))
print(list(mapping.items()))
print(view.tobytes())
print(view.tolist())
print(view.hex())
print(view.cast("B").tobytes())
view.release()
mutable_view = memoryview(bytearray(b"ab"))
mutable_view[0] = 122
print(mutable_view.tobytes())
print(mutable_view[1:].tobytes())
print(abs(-4))
print(all([1, 2]))
print(any([0, 2]))
print(bin(9), hex(26), oct(9), chr(65), ord("A"))
print(divmod(17, 5), pow(2, 5), sum([1, 2, 3]), min(4, 2, 3), max([4, 2, 3]))
members = set([1, 2])
members.add(3)
members.discard(2)
print(members)
print(list(enumerate(["a", "b"], 3)))
print(list(zip([1, 2], [3, 4])))
print(list(map(abs, [-1, 2])))
print(list(filter(None, [0, 1, 0, 2])))
print(sorted([3, 1, 2]), sorted([3, 1, 2], reverse=True))
print(id(payload) == id(payload))

values = [0, 1, 2, 3]
values[1:3] = [9]
print(values)
del values[1:2]
print(values)
