float_values = [
    0.0,
    -0.0,
    1.0,
    0.0001,
    0.00001,
    1000000000000000.0,
    10000000000000000.0,
    1.2345678901234567,
    1.2e20,
    1.2e-7,
    float("inf"),
    float("-inf"),
    float("nan"),
]
for value in float_values:
    print("repr", repr(value))

for value in [
    complex(0, 1),
    complex(-0.0, 1),
    complex(1, -0.0),
    complex(float("inf"), float("-inf")),
    complex(float("nan"), float("nan")),
    complex(1e16, 1e-5),
]:
    print("crepr", repr(value))

for value in ["\u00a0", "\u200b", "\u2028", "\u2060", "\ufeff", "😀", "\u0085", "\u00ad", "\u0378", "é"]:
    print("text", repr(value), ascii(value))

cycle = {}
values_view = cycle.values()
cycle["v"] = values_view
print("viewcycle", repr(values_view))
items_cycle = {}
items_view = items_cycle.items()
items_cycle["i"] = items_view
print("itemcycle", repr(items_view))

for spec in ["", "d", "+08d", "08,d", "08_d", "_d", "#x", "#_x", ",d", "#d", "0=8,d", "#08_X", "_b", "n", "z", ",n", "_n"]:
    try:
        print("intfmt", spec, format(1234, spec))
    except ValueError:
        print("intfmt-error", spec)

for value in [0.0, 1.0, 10.0, 100000.0, 1e-5]:
    for spec in [".0", ".1", ".2", ".6", "#", "#.6", "010"]:
        print("float-default", repr(value), spec, format(value, spec))

for value in [-0.0, -0.0001, 1.23456789, 1e16, float("inf"), float("nan")]:
    for spec in ["", "g", ".6g", "#.6g", "f", "+.2f", "z.2f", "+z.2f", "08.2f", "010,.2f", "_.2f", ".3e", ".3g", "n", "%"]:
        print("floatfmt", repr(value), spec, format(value, spec))

for value in [2.675, 1.005, 9.9995, 9.99995, 0.000099995, 99999.5, 999999.5]:
    for spec in [".2f", ".3g", ".4g", ".3e", ".6"]:
        print("float-round", repr(value), spec, format(value, spec))

for value in [float("inf"), float("nan")]:
    for spec in ["F", "E", "G"]:
        print("float-upper", repr(value), spec, format(value, spec))

for value in [complex(1, 2), complex(-0.0, -0.0), complex(0, 1)]:
    for spec in ["", ".6", "#.6", ".2f", ".3g", "+20.2f", "z.1f", "#.3g", ",.2f", "20", "+.6"]:
        print("complexfmt", repr(value), spec, format(value, spec))
for spec in ["020.2f", "=20.2f", "%", "#x", ",n"]:
    try:
        format(complex(1, 2), spec)
    except ValueError:
        print("complexfmt-error", spec)

for spec in ["", ">8", "*^9", ".2s", "05s", ">05s"]:
    print("strfmt", spec, format("abc", spec))
for spec in ["=.3s", "+s", "#s", ",s"]:
    try:
        format("abc", spec)
    except ValueError:
        print("strfmt-error", spec)

class Custom:
    def __repr__(self):
        return "CUSTOM-é"

    def __format__(self, spec):
        return "custom:" + spec

print("custom", repr(Custom()), ascii(Custom()), format(Custom(), "ok"))

class BadRepr:
    def __repr__(self):
        return 1

class BadStr:
    def __str__(self):
        return 1

class BadFormat:
    def __format__(self, spec):
        return 1

class Plain:
    pass

try:
    repr(BadRepr())
except TypeError:
    print("bad-repr-type")
try:
    str(BadStr())
except TypeError:
    print("bad-str-type")
try:
    format(BadFormat(), "x")
except TypeError:
    print("bad-format-type")
try:
    format(Plain(), "x")
except TypeError:
    print("plain-format-type")
