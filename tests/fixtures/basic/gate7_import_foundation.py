import inspect
import inspect as alias
import weakref as wr

print(inspect is alias)
print(inspect.__name__)
print(wr.__name__)
print(vars(inspect)["__name__"])

inspect.marker = 7
print(inspect.marker, vars(inspect)["marker"])
del inspect.marker
print("marker" in vars(inspect))


def load():
    import inspect as local
    return local is inspect, locals()["local"] is inspect


print(load())


class Holder:
    import weakref as local


print(Holder.local is wr)
