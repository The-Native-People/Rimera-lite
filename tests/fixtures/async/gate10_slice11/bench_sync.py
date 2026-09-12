ITERATIONS = 250000


def leaf(value):
    return value + 1


def main():
    index = 0
    total = 0
    while index < ITERATIONS:
        total += leaf(index)
        index += 1
    return total


print(main())
