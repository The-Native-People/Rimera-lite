events = []

class Manager:
    def __init__(self, name, value=None, suppress=False, exit_error=None):
        self.name = name
        self.value = value
        self.suppress = suppress
        self.exit_error = exit_error

    def __enter__(self):
        events.append("enter:" + self.name)
        return self.value

    def __exit__(self, exc_type, exc, tb):
        if exc_type is None:
            events.append("exit:" + self.name + ":none")
        else:
            events.append("exit:" + self.name + ":" + exc_type.__name__)
        if exc_type is None:
            events.append(tb is None)
        else:
            events.append(tb is not None)
        if self.exit_error is not None:
            raise self.exit_error
        return self.suppress

with Manager("name", 7) as number:
    events.append(number)

class Box:
    pass

box = Box()
values = [0]
with Manager("attribute", 11) as box.value:
    events.append(box.value)
with Manager("item", 13) as values[0]:
    events.append(values[0])
with Manager("unpack", (1, 2, 3, 4)) as (first, *middle, last):
    events.append((first, middle, last))
print(events)

events = []
with Manager("outer", "A") as outer, Manager("inner", "B") as inner:
    events.append(outer + inner)
with (
    Manager("parenthesized-outer", 2) as left,
    Manager("parenthesized-inner", 3) as right,
):
    events.append(left + right)
print(events)

events = []
class EnterFailure:
    def __enter__(self):
        events.append("enter:failure")
        raise ValueError("entry failed")

    def __exit__(self, exc_type, exc, tb):
        events.append("never-exit")

try:
    with Manager("entered"), EnterFailure():
        events.append("never-body")
except ValueError as error:
    events.append(str(error))
print(events)

events = []
class BrokenTarget:
    def __setitem__(self, key, value):
        events.append("target-set:" + key)
        raise ValueError("target failed")

broken = BrokenTarget()
try:
    with Manager("target", 23) as broken["slot"]:
        events.append("never-body")
except ValueError as error:
    events.append(str(error))
print(events)

events = []
with Manager("suppress-entry", suppress=True), EnterFailure():
    events.append("never-body")
events.append("suppressed")
print(events)

events = []
try:
    with Manager("replace-entry", exit_error=RuntimeError("replacement")), EnterFailure():
        events.append("never-body")
except RuntimeError as error:
    events.append(str(error))
    events.append(type(error.__context__).__name__)
print(events)
