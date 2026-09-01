builtin_print = print
builtin_len = len
builtin_range = range

len = lambda value: 41
range = lambda stop: [9, 8]
print = lambda value: builtin_print("shadow", value)

builtin_print(len([1, 2, 3]))
builtin_print(range(3))
print("print")
builtin_print(builtin_len([1, 2, 3]))
builtin_print(list(builtin_range(3)))

def local_shadow():
    len = lambda value: 7
    return len([1, 2])


def closure_shadow():
    len = lambda value: 8
    def inner():
        return len([1])
    return inner()

builtin_print(local_shadow())
builtin_print(closure_shadow())
