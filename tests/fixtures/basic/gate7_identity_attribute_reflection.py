events = []


class Meta(type):
    def __instancecheck__(cls, obj):
        events.append(("instance", cls.__name__, type(obj).__name__))
        return "truthy"

    def __subclasscheck__(cls, sub):
        events.append(("subclass", cls.__name__, sub.__name__))
        return []


class Target(metaclass=Meta):
    pass


class Child(Target):
    pass


class Other:
    pass


print(isinstance(Target(), Target), events)
print(isinstance(Child(), Target), events)
print(isinstance(Other(), Target), events)
print(issubclass(Target, Target), events)
print(issubclass(Child, Target), events)
print(isinstance(Other(), (int, Target)), events)

saved = AttributeError("saved attribute")


class Fallback:
    def __getattribute__(self, name):
        raise saved

    def __getattr__(self, name):
        return "fallback:" + name


fallback = Fallback()
print(getattr(fallback, "value", "default"), hasattr(fallback, "value"))

saved_identity = AttributeError("identity attribute")


class Bare:
    def __getattribute__(self, name):
        raise saved_identity


bare = Bare()
try:
    getattr(bare, "value")
except AttributeError as error:
    print(error is saved_identity, str(error))
print(getattr(bare, "value", "default"), hasattr(bare, "value"))


class Explode:
    def __getattribute__(self, name):
        raise ValueError("explode:" + name)


explode = Explode()
for helper in ["getattr", "hasattr"]:
    try:
        if helper == "getattr":
            getattr(explode, "value", "default")
        else:
            hasattr(explode, "value")
    except ValueError as error:
        print(helper, type(error).__name__, str(error))


class CallDescriptor:
    def __get__(self, obj, owner):
        print("BOUND CALL")
        raise RuntimeError("call binding")


class CallableProbe:
    __call__ = CallDescriptor()


callable_probe = CallableProbe()
print("callable", callable(callable_probe))
try:
    callable_probe()
except RuntimeError as error:
    print(type(error).__name__, str(error))


class AttrMeta(type):
    def __getattribute__(cls, name):
        if name == "virtual":
            return "meta-get"
        raise AttributeError("meta-miss")

    def __getattr__(cls, name):
        return "meta-fallback:" + name

    def __setattr__(cls, name, value):
        print("meta-set", name, value)

    def __delattr__(cls, name):
        print("meta-del", name)


class Managed(metaclass=AttrMeta):
    pass


print(getattr(Managed, "virtual"))
print(getattr(Managed, "missing"))
setattr(Managed, "created", 7)
delattr(Managed, "created")


class Reflect:
    def __repr__(self):
        return "répr"

    def __format__(self, spec):
        return "format:" + spec

    def __hash__(self):
        return 123


reflect = Reflect()
print(repr(reflect), ascii(reflect), format(reflect, "x"), hash(reflect))


class BadRepr:
    def __repr__(self):
        return 1


class BadFormat:
    def __format__(self, spec):
        return 1


class BadHash:
    def __hash__(self):
        return "bad"


for value, operation in [(BadRepr(), "repr"), (BadFormat(), "format"), (BadHash(), "hash")]:
    try:
        if operation == "repr":
            repr(value)
        elif operation == "format":
            format(value)
        else:
            hash(value)
    except TypeError as error:
        print(operation, type(error).__name__)

first = []
alias = first
second = []
first_id = id(first)
print(first_id == id(alias), first_id != id(second), type(first_id).__name__)
for index in range(200):
    pressure = [index, index + 1, index + 2]
print(first_id == id(first))

get = getattr
check = isinstance
print(get(reflect, "missing", 9), check(reflect, Reflect))
