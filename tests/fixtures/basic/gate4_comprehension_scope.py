events = []


def mark(label, value):
    events.append(label)
    return value


outer_value = 100
numbers = [1, 2, 3]
result = [
    mark("element", outer_value + item)
    for item in mark("outer-iterable", numbers)
    if mark("filter", item != 2)
]
print(result)
print(events)
print(outer_value)
try:
    print(item)
except NameError:
    print("target-does-not-leak")


class_scope_value = "global"


class ScopeProbe:
    class_scope_value = "class"
    body = [class_scope_value for ignored in [0]]
    outer = [value for value in [class_scope_value]]


print(ScopeProbe.body)
print(ScopeProbe.outer)

module_walrus = -1
[(module_walrus := value) for value in [4, 5, 6]]
print(module_walrus)


def closure_case(base):
    local_walrus = -1
    values = [(local_walrus := base + item) for item in [1, 2, 3]]
    nested = [[(local_walrus := left + right) for right in [10, 20]] for left in [1, 2]]
    return values, nested, local_walrus


print(closure_case(7))

global_walrus = -1


def update_global():
    global global_walrus
    [(global_walrus := item * 10) for item in [3, 4]]


update_global()
print(global_walrus)
