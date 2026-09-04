module_globals = globals()
print(module_globals is globals())
module_globals["published"] = 41
print(published)
print(locals() is module_globals)
print(vars() is module_globals)
print(dir() == sorted(dir()))


def ordered(z, a):
    q = 1
    b = 2
    return list(locals())


print(ordered(9, 8))


def snapshot(value):
    current = 1
    first = locals()
    first["value"] = 99
    first["extra"] = "kept"
    print(value, first["value"])
    current = 2
    second = locals()
    print(first is second, second["value"], second["current"], second["extra"])
    return second


retained = snapshot(7)
print(retained["value"], retained["current"], retained["extra"])


def vars_local():
    x = 1
    return vars() is locals()


print(vars_local())


class Namespace:
    view = locals()
    view["injected"] = 5
    print(locals() is view, injected)


class_view = vars(Namespace)
print(type(class_view).__name__, class_view["injected"], class_view is vars(Namespace))
instance = Namespace()
instance.a = 1
instance_view = vars(instance)
print(instance_view is vars(instance), instance_view["a"])
instance_view["b"] = 2
print(instance.b)


class SlotOnly:
    __slots__ = ("x",)


try:
    vars(SlotOnly())
except TypeError as error:
    print(type(error).__name__, str(error))


class Parent:
    parent = 1


class Child(Parent):
    child = 2


child = Child()
child.instance = 3
child_names = dir(child)
print(child_names == sorted(child_names), "parent" in child_names, "child" in child_names, "instance" in child_names)


class CustomDir:
    def __dir__(self):
        return ["z", "a", "z"]


print(dir(CustomDir()))


class IntegerDir:
    def __dir__(self):
        return [1]


print(dir(IntegerDir()))


class BadDir:
    def __dir__(self):
        return 1


try:
    dir(BadDir())
except TypeError as error:
    print(type(error).__name__)


def no_arg_dir():
    z = 1
    a = 2
    return dir()


print(no_arg_dir())


def comprehension_scope():
    outer = "outer"
    values = [(i, locals()["i"], locals()["outer"]) for i in [2]]
    nested = [(i, j, locals()["i"], locals()["j"], locals()["outer"]) for i in [3] for j in [4]]
    return values, nested, "i" in locals(), "j" in locals(), outer


print(comprehension_scope())


class ClassComprehension:
    base = 5
    values = [(i, locals()["base"], locals()["i"]) for i in [1]]


print(ClassComprehension.values, "i" in vars(ClassComprehension))


def generator_scope():
    a = 1
    first = locals()
    yield first["a"], first is locals()
    a = 2
    second = locals()
    yield first is second, second["a"]


generator = generator_scope()
print(next(generator))
print(next(generator))


def generator_expression_scope():
    y = "Y"
    z = "Z"
    return ((y, locals()) for i in [1])


genexpr = generator_expression_scope()
y_value, genexpr_locals = next(genexpr)
print(y_value, list(genexpr_locals), genexpr_locals["i"], genexpr_locals["y"], "z" in genexpr_locals)
