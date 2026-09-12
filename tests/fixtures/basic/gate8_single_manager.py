events = []

class MethodDescriptor:
    def __init__(self, name):
        self.name = name

    def __get__(self, obj, owner=None):
        events.append("get:" + self.name)
        name = self.name
        def bound(*args):
            events.append("call:" + name + ":" + str(len(args)))
            if name == "exit":
                events.append(args[0] is None)
                events.append(args[1] is None)
                events.append(args[2] is None)
            return None
        return bound

class DescriptorManager:
    __enter__ = MethodDescriptor("enter")
    __exit__ = MethodDescriptor("exit")

manager = DescriptorManager()
alias = manager
with alias:
    events.append("body")
print(events)

events = []
class BaseManager:
    def __enter__(self):
        events.append("base-enter")
        return self

    def __exit__(self, exc_type, exc, tb):
        events.append("base-exit")
        events.append(exc_type is None)
        return False

class ChildManager(BaseManager):
    pass

with ChildManager():
    events.append("child-body")
print(events)

events = []
class OldExit:
    def __get__(self, obj, owner=None):
        events.append("get:old-exit")
        def exit(exc_type, exc, tb):
            events.append("call:old-exit")
            return False
        return exit

class NewExit:
    def __get__(self, obj, owner=None):
        events.append("get:new-exit")
        def exit(exc_type, exc, tb):
            events.append("call:new-exit")
            return False
        return exit

class MutatingEnter:
    def __get__(self, obj, owner=None):
        events.append("get:mutating-enter")
        def enter():
            events.append("call:mutating-enter")
            MutatingManager.__exit__ = NewExit()
            return obj
        return enter

class MutatingManager:
    __enter__ = MutatingEnter()
    __exit__ = OldExit()

with MutatingManager():
    events.append("mutating-body")
print(events)

events = []
class ContextMeta(type):
    def __enter__(cls):
        events.append("meta-enter")
        return cls

    def __exit__(cls, exc_type, exc, tb):
        events.append("meta-exit")
        events.append(exc_type is None)
        return False

class ManagedType(metaclass=ContextMeta):
    pass

with ManagedType:
    events.append("meta-body")
print(events)
