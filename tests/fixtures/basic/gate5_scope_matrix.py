module_shadow = "module"
module_target = "start"
temporary_global = "alive"


class Base:
    def label(self):
        return "base"


def factory(seed):
    outer = seed
    shared = seed * 10
    local_shadow = "outer-shadow"

    def read_outer():
        return outer, shared

    def bump_shared():
        nonlocal shared
        shared += 1
        return shared

    capture = lambda value: outer + value
    list_values = [outer + value for value in [1, 2]]
    generator_values = tuple(outer + value for value in [3, 4])

    match [seed, seed + 1]:
        case [first, second]:
            matched = first + second

    class Local(Base):
        before_shadow = module_shadow
        module_shadow = "class"
        outer_seen = outer

        nonlocal shared
        shared += 5

        global module_target
        module_target = "class-global"

        class_local = 100
        comprehension = [outer + value for value in [5, 6]]
        local_comprehension = [local_shadow for ignored in [0]]

        class Nested:
            nested_outer = outer

        def method(self):
            return module_shadow, outer, shared

        def label(self):
            return super().label(), outer

        def class_name(self):
            return __class__.__name__

    return (
        Local,
        read_outer,
        bump_shared,
        capture,
        list_values,
        generator_values,
        matched,
        shared,
    )


Local, read_outer, bump_shared, capture, list_values, generator_values, matched, shared = factory(7)
print(Local.before_shadow, Local.module_shadow, Local.outer_seen)
print(Local.comprehension, Local.local_comprehension, Local.Nested.nested_outer)
print(hasattr(Local, "shared"), module_target)
print(Local().method())
print(Local().label())
print(Local().class_name())
print(read_outer(), bump_shared(), read_outer())
print(capture(3), list_values, generator_values, matched, shared)


def sibling_cells():
    value = 0

    def increment():
        nonlocal value
        value += 1
        return value

    def read():
        return value

    return increment, read


increment, read = sibling_cells()
print(read(), increment(), read(), increment(), read())


def delete_and_rebind():
    value = "alive"

    def read():
        return value

    print(read())
    del value
    try:
        read()
    except NameError as error:
        print(type(error).__name__, str(error))
    value = "reborn"
    print(read())


delete_and_rebind()


def remove_global():
    global temporary_global
    del temporary_global


remove_global()
try:
    print(temporary_global)
except NameError as error:
    print(type(error).__name__, str(error))


def annotation_local():
    annotated: int
    try:
        return annotated
    except UnboundLocalError as error:
        return type(error).__name__, str(error)


print(annotation_local())


def recursive(depth):
    activation = depth
    if depth == 0:
        return activation
    return activation, recursive(depth - 1)


print(recursive(4))


outer_for_class = "outer"


def class_comprehension_scope():
    outer_for_class = "enclosing"

    class Probe:
        outer_for_class = "class"
        values = [outer_for_class for ignored in [0]]

        def read(self):
            return outer_for_class

    return Probe


Probe = class_comprehension_scope()
print(Probe.outer_for_class, Probe.values, Probe().read())


def walrus_owner():
    marker = -1
    values = [(marker := value) for value in [8, 9]]
    return values, marker


print(walrus_owner())
