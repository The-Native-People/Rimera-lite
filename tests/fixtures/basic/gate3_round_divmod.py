round_cases = [
    (2.675, 2),
    (1.2345, 3),
    (-0.0, 2),
    (-0.0001, 2),
    (1.2345, -2),
    (250.0, -2),
    (350.0, -2),
    (5e-324, 323),
    (5e-324, 324),
    (1e-320, 322),
    (1e308, -308),
    (1e308, -309),
]
for case in round_cases:
    value = case[0]
    ndigits = case[1]
    result = round(value, ndigits)
    print("round", repr(value), ndigits, type(result).__name__, repr(result))

print("round-large", type(round(1e20)).__name__, round(1e20))
print("round-ints", round(12345, -2), round(12500, -3), round(13500, -3), round(-12500, -3))
print("round-extreme-ndigits", repr(round(1.0, 10 ** 100)), repr(round(-1.0, -(10 ** 100))))
try:
    round(1.79e308, -308)
except OverflowError:
    print("round-overflow")

for value in [float("inf"), float("-inf"), float("nan")]:
    try:
        round(value)
    except Exception as error:
        print("round-special-error", repr(value), type(error).__name__)
    print("round-special-explicit", repr(value), repr(round(value, 2)))

class CustomRound:
    def __round__(self, ndigits=None):
        return ("custom", ndigits)

print("round-custom", round(CustomRound()), round(CustomRound(), 3), round(number=CustomRound(), ndigits=4))

divmod_cases = [
    (3.0, 2.0),
    (-3.0, 2.0),
    (3.0, -2.0),
    (-3.0, -2.0),
    (0.0, 2.0),
    (-0.0, 2.0),
    (0.0, -2.0),
    (-0.0, -2.0),
    (1e308, 3.0),
    (1e-308, 3.0),
    (1.0, 0.1),
    (1.0, -0.1),
    (2.0, float("inf")),
    (2.0, float("-inf")),
    (float("inf"), 2.0),
    (float("nan"), 2.0),
    (3, 2.0),
]
for case in divmod_cases:
    left = case[0]
    right = case[1]
    result = divmod(left, right)
    quotient = result[0]
    remainder = result[1]
    print("divmod", repr(left), repr(right), repr(quotient), repr(remainder))

print("divmod-bigint", divmod(10 ** 100, 3))
try:
    divmod(1.0, 0.0)
except ZeroDivisionError:
    print("divmod-zero")
try:
    divmod(10 ** 400, 2.0)
except OverflowError:
    print("divmod-overflow")

class Base:
    def __divmod__(self, other):
        return ("base-direct",)

class Sub(Base):
    def __rdivmod__(self, other):
        return ("sub-reflected",)

class Left:
    def __divmod__(self, other):
        return NotImplemented

class Right:
    def __rdivmod__(self, other):
        return ("reflected",)

print("divmod-priority", divmod(Base(), Sub()))
print("divmod-fallback", divmod(Left(), Right()))
