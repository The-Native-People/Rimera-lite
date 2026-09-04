def make(seed):
    captured = [seed]

    def function(value):
        return captured[0] + value

    return function.__code__, function.__closure__


for index in range(180):
    dead_code, dead_closure = make(index)
    dead_code = None
    dead_closure = None
    pressure = [index, index + 1, index + 2, index + 3]

code, closure = make(999)
for index in range(180):
    pressure = [index, index + 1, index + 2, index + 3]

print(code.co_name, code.co_qualname)
print(code.co_argcount, code.co_nlocals, code.co_varnames)
print(code.co_cellvars, code.co_freevars, code.co_flags)
print(closure is closure, len(closure), closure[0].cell_contents[0])
closure[0].cell_contents = [1000]
print(closure[0].cell_contents[0])
