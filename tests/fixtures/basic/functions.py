def add(a, b=2, *rest, scale=1, **named):
    return (a + b) * scale


def factorial(n):
    if n <= 1:
        return 1
    return n * factorial(n - 1)


def make_counter(start):
    def advance(amount):
        nonlocal start
        start = start + amount
        return start

    return advance


print(add(40))
print(add(20, 1, 99, scale=2, ignored=7))
print(factorial(10))
counter = make_counter(10)
print(counter(2), counter(3))
