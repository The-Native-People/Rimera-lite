s = slice(-100, 100, 3)
print("slice-fields", s.start, s.stop, s.step)
print("slice-none", slice(None).start, slice(None).stop, slice(None).step)
print("slice-indices", s.indices(5), slice(None, None, -1).indices(5))
print("slice-huge", slice(-100, 100, 3).indices(10 ** 100))

class IndexLength:
    def __index__(self):
        return 5

print("slice-index-like", slice(1, 9, 2).indices(IndexLength()))
try:
    slice(None).indices(-1)
except ValueError:
    print("slice-negative-length")
try:
    slice(None, None, 0).indices(5)
except ValueError:
    print("slice-zero-step")
try:
    s.start = 1
except AttributeError:
    print("slice-immutable")

formats = ["l", "L", "n", "N", "P", "@l", "@L", "@n", "@N", "@P"]
for fmt in formats:
    backing = bytearray(16)
    view = memoryview(backing).cast(fmt)
    code = fmt[-1]
    value = 5
    if code == "l" or code == "n":
        value = -5
    view[0] = value
    print("memory-format", fmt, view.format, view.itemsize, view[0], view.tolist()[0])

positive_unsigned = memoryview(bytes([1, 2])).cast("B")
positive_signed = memoryview(bytes([1, 2])).cast("b")
negative_unsigned = memoryview(bytes([255])).cast("B")
negative_signed = memoryview(bytes([255])).cast("b")
print("memory-equality", positive_unsigned == positive_signed, negative_unsigned == negative_signed)
integer_view = memoryview(bytearray(4)).cast("i")
float_view = memoryview(bytearray(4)).cast("f")
integer_view[0] = 1
float_view[0] = 1.0
print("memory-numeric-equality", integer_view == float_view)
float_view[0] = 1.5
print("memory-numeric-inequality", integer_view == float_view)

readonly_bytes = memoryview(b"abcdef").cast("@B")
print("memory-hash", hash(readonly_bytes) == hash(readonly_bytes.tobytes()))
mutable = bytearray(b"abcdef")
nested_readonly = memoryview(memoryview(mutable).toreadonly()).toreadonly()
try:
    hash(nested_readonly)
except TypeError:
    print("memory-unhashable-exporter")

base = memoryview(b"abcdef")
noncontiguous = base[slice(None, None, 2)]
print("memory-noncontiguous", bytes(noncontiguous), bytearray(noncontiguous), noncontiguous.tobytes())

typed = memoryview(bytes(16)).cast("n")
print("memory-raw-conversion", len(bytes(typed)), len(bytearray(typed)), typed.tolist())
