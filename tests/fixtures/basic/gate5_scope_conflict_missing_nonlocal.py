def outer():
    def inner():
        nonlocal missing
        return missing
    return inner
