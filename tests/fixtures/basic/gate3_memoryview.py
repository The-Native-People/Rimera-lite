backing = bytearray(24)
view = memoryview(backing)
matrix = view.cast(format="i", shape=[2, 3])
print(len(matrix), matrix.ndim, matrix.shape, matrix.strides, matrix.nbytes)
print(matrix.tolist())
matrix[1, 2] = 7
print(matrix[1, 2], matrix.tolist())
first = matrix[0:1]
print(first.shape, first.strides, first.tolist())
reverse = matrix[::-1]
print(reverse.shape, reverse.strides, reverse.tolist())
flat = matrix.cast("B")
print(len(flat), flat.itemsize, flat.format)
print(list(memoryview(backing).cast("i")))
print(view.obj is backing, view.suboffsets, view.c_contiguous, view.f_contiguous, view.contiguous)
print(first.c_contiguous, first.f_contiguous, first.contiguous)
print(reverse.c_contiguous, reverse.f_contiguous, reverse.contiguous)
readonly = view.toreadonly()
print(readonly.readonly, readonly.obj is backing, readonly.tolist() == view.tolist())
print(view.hex(":", 2))
print(view.hex(sep=":", bytes_per_sep=-3))
print(matrix.tobytes("C") == matrix.tobytes(order="C"))
print(len(matrix.tobytes("F")), len(matrix.tobytes("A")))
scalar = memoryview(bytearray(4)).cast("i", shape=[])
scalar[()] = 12
print(scalar.ndim, scalar.shape, scalar.tolist(), scalar[()])
try:
    print(len(scalar))
except TypeError as error:
    print(type(error).__name__, error)
print(hash(memoryview(b"abc")) == hash(b"abc"))
try:
    print(hash(readonly))
except TypeError as error:
    print(type(error).__name__, error)
