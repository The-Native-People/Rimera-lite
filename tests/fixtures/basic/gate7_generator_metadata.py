current = None


def probe(seed):
    local = seed + 1
    print("inside", current.gi_running, current.gi_suspended)
    received = yield local
    yield received


current = probe(4)
print(current.__name__, current.__qualname__)
print(current.gi_code is probe.__code__)
print(current.gi_frame is current.gi_frame, current.gi_frame.f_code is current.gi_code)
print(current.gi_frame.f_globals is globals(), current.gi_frame.f_back is None)
print(current.gi_frame.f_locals["seed"])
print(current.gi_running, current.gi_suspended, current.gi_yieldfrom)
print(next(current))
print(current.gi_running, current.gi_suspended, current.gi_yieldfrom)
print(current.gi_frame.f_locals["seed"], current.gi_frame.f_locals["local"])
print(current.gi_frame.f_lineno)
print(current.send(9))
print(current.gi_frame.f_locals["received"])

current.__name__ = "renamed"
current.__qualname__ = "renamed.qual"
print(current.__name__, current.__qualname__, probe.__name__, probe.__qualname__)

for attribute in ["gi_code", "gi_frame", "gi_running", "gi_suspended", "gi_yieldfrom"]:
    try:
        setattr(current, attribute, None)
    except AttributeError as error:
        print(attribute, type(error).__name__)

try:
    current.__name__ = 1
except TypeError as error:
    print("name", type(error).__name__)

current.close()
print(current.gi_frame is None, current.gi_running, current.gi_suspended, current.gi_yieldfrom)


def child():
    yield 3
    return 7


def delegator():
    result = yield from child()
    yield result


delegated = delegator()
print(delegated.gi_yieldfrom is None, delegated.gi_suspended)
print(next(delegated))
delegate = delegated.gi_yieldfrom
print(type(delegate).__name__, delegate.gi_suspended, delegated.gi_suspended)
print(next(delegated))
print(delegated.gi_yieldfrom is None, delegated.gi_suspended)
delegated.close()
print(delegated.gi_frame is None, delegated.gi_yieldfrom is None)


def completed():
    yield 1


done = completed()
print(next(done), done.gi_suspended)
try:
    next(done)
except StopIteration:
    print("done")
print(done.gi_frame is None, done.gi_suspended, done.gi_running)


def failed():
    yield 1
    raise ValueError("failure")


broken = failed()
print(next(broken), broken.gi_suspended)
try:
    next(broken)
except ValueError as error:
    print(type(error).__name__, str(error))
print(broken.gi_frame is None, broken.gi_suspended, broken.gi_running)


fresh = completed()
fresh.close()
print(fresh.gi_frame is None, fresh.gi_suspended, fresh.gi_running)

