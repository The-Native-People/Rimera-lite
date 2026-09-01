def mark(label):
    print(label)

try:
    [1][4]
except IndexError:
    mark("index-error")

bad_index = "x"
try:
    [1][bad_index]
except TypeError:
    mark("index-type")

try:
    {"a": 1}["missing"]
except KeyError:
    mark("dict-key")

try:
    {}.pop("missing")
except KeyError:
    mark("dict-pop")

try:
    {1}.remove(2)
except KeyError:
    mark("set-remove")

buffer = bytearray([1, 2, 3])
view = memoryview(buffer)
try:
    buffer[1:1] = bytes([9])
except BufferError:
    mark("buffer-resize")
view.release()

try:
    hash(memoryview(bytearray([1])))
except ValueError:
    mark("memoryview-hash")

hash_buffer = bytearray([1])
hash_view = memoryview(hash_buffer)
readonly_hash_view = hash_view.toreadonly()
try:
    hash(readonly_hash_view)
except TypeError:
    mark("memoryview-unhashable")
readonly_hash_view.release()
hash_view.release()

try:
    memoryview(b"abcd").cast("z")
except ValueError:
    mark("memoryview-format")

try:
    memoryview(b"abcd").cast("B", shape="bad")
except TypeError:
    mark("memoryview-shape")

try:
    [1, 2, 3][slice(None, None, 0)]
except ValueError:
    mark("slice-zero")

try:
    chr(0x110000)
except ValueError:
    mark("chr-range")

try:
    int("bad")
except ValueError:
    mark("int-literal")

try:
    int("10", 1)
except ValueError:
    mark("int-base")

try:
    int(float("inf"))
except OverflowError:
    mark("int-inf")

try:
    int(float("nan"))
except ValueError:
    mark("int-nan")

try:
    float("bad")
except ValueError:
    mark("float-text")

try:
    complex("bad")
except ValueError:
    mark("complex-text")

try:
    bytes([256])
except ValueError:
    mark("bytes-range")

try:
    pow(2, 3, 0)
except ValueError:
    mark("pow-mod-zero")

try:
    pow(2, -1, 4)
except ValueError:
    mark("pow-inverse")

try:
    [1].index(2)
except ValueError:
    mark("list-index")

try:
    [1].remove(2)
except ValueError:
    mark("list-remove")

try:
    [1].pop(9)
except IndexError:
    mark("list-pop")

try:
    format(1, ".2d")
except ValueError:
    mark("format-value")

class IndexBomb:
    def __index__(self):
        raise ValueError("index callback")

try:
    [1][IndexBomb()]
except ValueError:
    mark("callback-preserved")

z = complex(1, 0)
print(z == 1, z == 1.0, hash(z) == hash(1))
print(z + 2, 2 + z, z - 2, 2 - z)
print(z + 2.5, 2.5 - z)
print(z * 2, 2 * z, z / 2, 2 / z)
print(+z, -z, abs(complex(3, 4)))
print(z ** 2, pow(z, 2))
w = complex(1, 2)
print(w * w, w ** 2)
print(complex(-0.0, 0) == complex(0.0, 0))
nan_complex = complex(float("nan"), 0)
print(nan_complex == nan_complex)
print(complex(1, 2) / complex(float("inf"), 0))

try:
    z // 2
except TypeError:
    mark("complex-floor")

try:
    z % 2
except TypeError:
    mark("complex-mod")

try:
    z < 2
except TypeError:
    mark("complex-order")

try:
    z / 0
except ZeroDivisionError:
    mark("complex-zero-div")

try:
    complex(0, 0) ** -1
except ZeroDivisionError:
    mark("complex-zero-power")

try:
    pow(z, 2, 3)
except ValueError:
    mark("complex-mod-pow")
