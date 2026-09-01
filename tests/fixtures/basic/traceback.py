def inner():
    return 1 // 0


def outer():
    return inner()


outer()
