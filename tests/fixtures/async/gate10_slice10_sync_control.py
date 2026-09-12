def child(value):
    return value + 1


def main():
    first = child(40)
    second = child(first)
    print("async-root", second)
    return second


print("root-return", main())
