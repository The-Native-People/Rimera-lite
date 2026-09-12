events = []

class Descriptor:
    def __get__(self, obj, owner):
        events.append(("get", owner.__name__))
        return owner.marker

class A:
    marker = "A"
    value = Descriptor()

    def method(self):
        return "A"

class B(A):
    pass

class C(B):
    pass

obj = C()
print(obj.method(), obj.value, "fresh" in dir(C), isinstance(obj, A), issubclass(C, A))

def method2(self):
    return "A2"

A.method = method2
A.marker = "A2"
A.fresh = 9
print(obj.method(), obj.value, "fresh" in dir(C), C.fresh)
del A.fresh
print("fresh" in dir(C), hasattr(C, "fresh"))

class X:
    marker = "X"
    value = Descriptor()

    def method(self):
        return "X"

B.__bases__ = (X,)
print(obj.method(), obj.value, isinstance(obj, A), isinstance(obj, X), issubclass(C, A), issubclass(C, X))
try:
    B.__bases__ = (int,)
except TypeError:
    print("bases rollback", obj.method(), issubclass(C, X))

class Meta(type):
    pass

class M(metaclass=Meta):
    pass

Meta.flag = "one"
print(M.flag)
Meta.flag = "two"
print(M.flag)
del Meta.flag
print(hasattr(M, "flag"))


def outer(seed):
    cell = seed

    def fn(delta=1):
        return cell + delta

    def gen():
        local = cell
        try:
            yield local
        except ValueError as error:
            events.append(("caught", str(error)))
            yield "handled"

    return fn, gen()

fn, gen = outer(5)
code = fn.__code__
closure = fn.__closure__
print(code.co_name, code.co_freevars, closure[0].cell_contents)
print(next(gen), gen.gi_suspended, gen.gi_frame.f_locals["local"])
print(gen.throw(ValueError("resume")), gen.gi_suspended)

class Generic[T]:
    token = T

param = Generic.__type_params__[0]
print(param.__name__, Generic.token is param)

class Provider:
    def __init__(self):
        self.data = bytearray(b"abcd")
        self.releases = 0

    def __buffer__(self, flags):
        self.inner = memoryview(self.data)
        return self.inner

    def __release_buffer__(self, view):
        self.releases += 1

provider = Provider()
view = memoryview(provider)
child = view[1:3]
view.release()
A.marker = "after-buffer"
print(child.tobytes(), obj.value, provider.releases)

for index in range(700):
    text = str(index) + "-reflection-composition-" + str(index)
    pair = (text, index)
    [pair, text, index]

print(code.co_name, closure[0].cell_contents, param.__name__, child.tobytes())
child.release()
print(provider.releases)
print(events)
