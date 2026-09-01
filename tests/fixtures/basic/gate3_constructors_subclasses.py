print(int("1_2"), int("0x_1", 0), int("00", 0), int("0_0", 0), int("１２", 10))
for text in ["1__2", "_12", "12_", "010"]:
    try:
        if text == "010":
            int(text, 0)
        else:
            int(text)
    except ValueError:
        print("int-error", text)
print(int(memoryview(b"12")))

print(float("1_2.5"), float("１２.５"), float("Infinity"), float("-inf"))
try:
    float("1__2.5")
except ValueError:
    print("float-error")
print(float(memoryview(b"1.25")))

class Floatish:
    def __float__(self):
        return 2.5

class Indexish:
    def __index__(self):
        return 3

print(float(Floatish()), float(Indexish()), int(Indexish()))

class BadFloat:
    def __float__(self):
        return 1

try:
    float(BadFloat())
except TypeError:
    print("float-type-error")

print(complex("1e-3j"), complex("(1_2+3j)"), complex("+j"), complex(1, 2))
try:
    complex("1 + 2j")
except ValueError:
    print("complex-error")

class Complexish:
    def __complex__(self):
        return complex(2, 3)

class BadComplex:
    def __complex__(self):
        return 1

print(complex(Complexish()))
try:
    complex(BadComplex())
except TypeError:
    print("complex-type-error")

view = memoryview(b"\x01\x00\x00\x00").cast("i")
print(bytes(view), bytearray(view))
print(bytes("é", "utf-8"), bytearray("é", "utf-8"))
print(bytes(3), bytearray([1, 2, 255]))
try:
    bytes([256])
except ValueError:
    print("bytes-range-error")

class F(float):
    pass

class C(complex):
    pass

class B(bytes):
    pass

class BA(bytearray):
    pass

class D(dict):
    pass

class S(set):
    pass

class FS(frozenset):
    pass

class T(tuple):
    pass

class L(list):
    pass

class InitL(list):
    def __init__(self, value):
        self.mark = "L"

class InitD(dict):
    def __init__(self, value):
        self.mark = "D"

class InitS(set):
    def __init__(self, value):
        self.mark = "S"

class InitBA(bytearray):
    def __init__(self, value):
        self.mark = "BA"

class InitT(tuple):
    def __init__(self, value):
        self.mark = "T"

class SuperL(list):
    def __init__(self, value):
        super().__init__(value)

class SuperD(dict):
    def __init__(self, value):
        super().__init__(value)

class SuperS(set):
    def __init__(self, value):
        super().__init__(value)

class SuperBA(bytearray):
    def __init__(self, value):
        super().__init__(value)

class MappingSource:
    def keys(self):
        return [1]

    def __getitem__(self, key):
        return "one"

f = F(Floatish())
f_index = F(Indexish())
c = C(1, 2)
c_text = C("1e-3j")
c_custom = C(Complexish())
b = B("é", "utf-8")
b_view = B(view)
ba = BA(memoryview(b"abc"))
ba_text = BA("é", "utf-8")
d = D(MappingSource(), x=2)
d_pairs = D([(3, "three")])
print("d-pairs-len", len(d_pairs))
print("d-pairs-type", type(d_pairs).__name__)
print("d-pairs-get", d_pairs[3])
s = S([1, 2, 1])
fs = FS([1, 2, 1])
t = T([3, 4])
l = L([5, 6])
init_l = InitL([1, 2])
init_d = InitD({"a": 1})
init_s = InitS([1, 2])
init_ba = InitBA(b"ab")
init_t = InitT([1, 2])
super_l = SuperL([7, 8])
super_d = SuperD({"z": 9})
super_s = SuperS([7, 8])
super_ba = SuperBA(b"xy")

print(type(f).__name__, f, f_index)
print(type(c).__name__, c, c_text, c_custom)
print(type(b).__name__, len(b), b[0], b[1], bytes(b_view))
print(type(ba).__name__, len(ba), ba[0], ba[2], bytes(ba_text))
print("sub-bytes-numeric", int(B(b"12")), int(B(b"ff"), 16), float(B(b"1.5")), int(BA(b"12")), float(BA(b"1.5")))
print(type(d).__name__, len(d))
print("d-one", d[1])
print("d-x", d["x"])
print("d-pairs-meta", type(d_pairs).__name__, len(d_pairs))
print("d-pairs", d_pairs[3])
print(type(s).__name__, len(s), 2 in s)
print(type(fs).__name__, len(fs), 2 in fs)
print(type(t).__name__, len(t), t[1])
print(type(l).__name__, len(l), l[0])
print(
    "custom-init",
    len(init_l), init_l.mark,
    len(init_d), init_d.mark,
    len(init_s), init_s.mark,
    len(init_ba), init_ba.mark,
    len(init_t), init_t.mark,
)
print(
    "super-init",
    list(super_l),
    list(super_d.items()),
    sorted(super_s),
    bytes(super_ba),
)
