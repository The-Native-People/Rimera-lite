events = []

class Manager:
    def __init__(self, name, fail=False):
        self.name = name
        self.fail = fail

    def __enter__(self):
        events.append("enter:" + self.name)
        return self.name

    def __exit__(self, exc_type, exc, tb):
        if exc_type is None:
            events.append("exit:" + self.name + ":none")
        else:
            events.append("exit:" + self.name + ":" + exc_type.__name__)
        if self.fail:
            raise RuntimeError("exit:" + self.name)
        return False

def returned():
    with Manager("return-outer"), Manager("return-inner"):
        events.append("return-body")
        return "returned"

print(returned())
print(events)

events = []
for value in range(4):
    with Manager("loop:" + str(value)):
        try:
            if value == 1:
                events.append("continue")
                continue
            if value == 2:
                events.append("break")
                break
            events.append("body:" + str(value))
        finally:
            events.append("finally:" + str(value))
print(events)

events = []
def nested_finally():
    try:
        with Manager("nested"):
            events.append("nested-body")
            return 31
    finally:
        events.append("outer-finally")

print(nested_finally())
print(events)

events = []
try:
    def replaced_return():
        with Manager("replace-return", fail=True):
            return 99
    replaced_return()
except RuntimeError as error:
    events.append(str(error))
print(events)

events = []
class ReentrantManager(Manager):
    def __exit__(self, exc_type, exc, tb):
        events.append("reentrant-start")
        with Manager("inside-exit"):
            events.append("inside-exit-body")
        events.append("reentrant-end")
        return False

with ReentrantManager("reentrant"):
    events.append("reentrant-body")
print(events)

events = []
class ClassManager(Manager):
    def __enter__(self):
        events.append("enter:" + self.name)
        return 41

class BuiltInsideWith:
    with ClassManager("class") as marker:
        doubled = marker + 1

print(BuiltInsideWith.marker, BuiltInsideWith.doubled)
print(events)
