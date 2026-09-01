value = 3.5
print(value.real is value, value.imag, value.conjugate() is value)
print(value.is_integer(), float(4).is_integer(), float("inf").is_integer(), float("nan").is_integer())
print(value.as_integer_ratio())
print(float("0.1").as_integer_ratio())
print(value.hex())
print(float("-0.0").hex())
print(float("5e-324").hex())
print(float("inf").hex(), float("-inf").hex(), float("nan").hex())

try:
    float("inf").as_integer_ratio()
except OverflowError as error:
    print(type(error).__name__, str(error))

try:
    float("nan").as_integer_ratio()
except ValueError as error:
    print(type(error).__name__, str(error))

number = complex(3, -4)
print(number.real, number.imag, number.conjugate())
