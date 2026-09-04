def eval(value):
    return "user-eval", value


def exec(value):
    return "user-exec", value


def compile(source, filename, mode):
    return "user-compile", source, filename, mode


print(eval(7))
print(exec(8))
print(compile("x", "file.py", "exec"))
