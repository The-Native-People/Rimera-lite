events = []


def mark(value):
    events.append(value)
    return value


def decorator(label):
    events.append(("decorator", label))

    def apply(function):
        events.append(("apply", label, function.__name__, function.__qualname__))
        return function

    return apply


@decorator(mark("D1"))
@decorator(mark("D2"))
def sample(value=mark(["mutable"]), *, flag=mark(20)) -> mark("RET"):
    return value, flag

print(events)
print(sample.__name__, sample.__qualname__)
print(sample.__defaults__, sample.__kwdefaults__, sample.__annotations__)
print(sample())

sample.__defaults__[0].append("changed")
sample.__kwdefaults__["flag"] = 21
print(sample())

sample.__defaults__ = (["replacement"],)
sample.__kwdefaults__ = {"flag": 22}
print(sample())

alias = sample
sample = "rebound"
print(alias.__name__, alias.__qualname__, alias())
alias.__name__ = "renamed"
alias.__qualname__ = "custom.path"
print(alias.__name__, alias.__qualname__)


def outer():
    @decorator("INNER")
    def inner():
        return lambda: 1

    direct = lambda: 2
    from_comprehension = [lambda: 3 for ignored in [0]][0]
    return inner, direct, from_comprehension


inner, direct, from_comprehension = outer()
print(outer.__qualname__)
print(inner.__qualname__)
print(direct.__qualname__)
print(from_comprehension.__qualname__)


class Holder:
    method_lambda = lambda: 4

    @decorator("METHOD")
    def method(self, value=mark(5), *, flag=mark(6)):
        def nested():
            return value

        return nested, flag


print(events)
holder = Holder()
nested, method_flag = holder.method()
print(Holder.method_lambda.__qualname__)
print(Holder.method.__name__, Holder.method.__qualname__)
print(Holder.method.__defaults__, Holder.method.__kwdefaults__)
print(nested.__qualname__, nested(), method_flag)


def metadata_target(value=10, *, flag=11):
    return value, flag


for action in ["name", "qualname", "defaults", "kwdefaults", "annotations"]:
    try:
        if action == "name":
            metadata_target.__name__ = 1
        elif action == "qualname":
            metadata_target.__qualname__ = 1
        elif action == "defaults":
            metadata_target.__defaults__ = []
        elif action == "kwdefaults":
            metadata_target.__kwdefaults__ = []
        else:
            metadata_target.__annotations__ = []
    except TypeError as error:
        print(action, type(error).__name__, str(error))

del metadata_target.__defaults__
del metadata_target.__kwdefaults__
del metadata_target.__annotations__
print(metadata_target.__defaults__, metadata_target.__kwdefaults__, metadata_target.__annotations__)
metadata_target.__defaults__ = (12,)
metadata_target.__kwdefaults__ = {"flag": 13}
print(metadata_target())


def replace(function):
    return lambda: "replacement"


@replace
def replaced():
    return "original"

print(replaced.__name__, replaced())

stable = "old"


def fail(function):
    raise RuntimeError("decorator boom")


try:
    @fail
    def stable():
        return "new"
except RuntimeError as error:
    print(type(error).__name__, str(error))

print(stable)

failure_events = []


def fail_expression():
    failure_events.append("decorator expression")
    raise RuntimeError("expression boom")


try:
    @fail_expression()
    def unpublished(value=mark("SHOULD NOT RUN")):
        return value
except RuntimeError as error:
    print(type(error).__name__, str(error))

print(failure_events)
