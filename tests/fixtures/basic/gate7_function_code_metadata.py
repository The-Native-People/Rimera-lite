def identity(function):
    return function


@identity
def outer(a: int = 1, /, b=2, *args, c: str = 3, **kwargs) -> bool:
    x = 4

    def inner(d):
        return a + x + d

    return inner


inner = outer()
print(outer.__name__, outer.__qualname__)
print(outer.__defaults__, outer.__kwdefaults__, outer.__annotations__)
print(outer.__closure__ is None)
print(outer.__code__ is outer.__code__)
print(inner.__closure__ is inner.__closure__, len(inner.__closure__))
print(inner.__closure__[0].cell_contents, inner.__closure__[1].cell_contents)

outer_code = outer.__code__
print(outer_code.co_name, outer_code.co_qualname)
print(outer_code.co_filename)
print(outer_code.co_firstlineno)
print(outer_code.co_argcount, outer_code.co_posonlyargcount, outer_code.co_kwonlyargcount)
print(outer_code.co_nlocals, outer_code.co_varnames)
print(outer_code.co_cellvars, outer_code.co_freevars, outer_code.co_flags)

inner_code = inner.__code__
print(inner_code.co_name, inner_code.co_qualname)
print(inner_code.co_firstlineno)
print(inner_code.co_argcount, inner_code.co_posonlyargcount, inner_code.co_kwonlyargcount)
print(inner_code.co_nlocals, inner_code.co_varnames)
print(inner_code.co_cellvars, inner_code.co_freevars, inner_code.co_flags)

cell = inner.__closure__[1]
cell.cell_contents = 10
print(inner(5))
del cell.cell_contents
try:
    print(cell.cell_contents)
except ValueError as error:
    print(type(error).__name__, str(error))
cell.cell_contents = 4
print(inner(5))

for action in ["closure-set", "closure-delete", "code-type"]:
    try:
        if action == "closure-set":
            inner.__closure__ = None
        elif action == "closure-delete":
            del inner.__closure__
        else:
            inner.__code__ = 1
    except (AttributeError, TypeError) as error:
        print(action, type(error).__name__, str(error))

try:
    outer.__code__ = inner.__code__
except ValueError as error:
    print(type(error).__name__, str(error))


def left(value):
    return value + 1


def right(value):
    return value + 10


left_code = left.__code__
right_code = right.__code__
left.__code__ = right_code
print(left(2), left.__name__, left.__qualname__, left.__code__.co_name)
left.__code__ = left_code
print(left(2), left.__code__.co_name)

alias = inner
print(alias.__code__ is inner.__code__, alias.__closure__ is inner.__closure__)


class Holder:
    def method(self, value=5, *, flag=6):
        return value + flag


print(Holder.method.__name__, Holder.method.__qualname__)
print(Holder.method.__defaults__, Holder.method.__kwdefaults__)
print(Holder.method.__code__.co_name, Holder.method.__code__.co_qualname)
print(Holder.method.__code__.co_firstlineno)
print(Holder.method.__code__.co_varnames, Holder.method.__code__.co_flags)


def make_lambdas():
    direct = lambda value: value + 1
    from_comprehension = [lambda value: value + 2 for ignored in [0]][0]
    return direct, from_comprehension


direct, from_comprehension = make_lambdas()
print(direct.__qualname__, direct.__code__.co_qualname, direct.__code__.co_firstlineno)
print(direct.__code__.co_varnames, direct.__code__.co_flags)
print(from_comprehension.__qualname__, from_comprehension.__code__.co_qualname)
print(from_comprehension.__code__.co_firstlineno, from_comprehension.__code__.co_varnames)
print(direct(3), from_comprehension(3))

try:
    outer_code.co_name = "changed"
except AttributeError as error:
    print(type(error).__name__, str(error))
