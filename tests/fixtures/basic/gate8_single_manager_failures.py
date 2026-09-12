def show(error):
    print(type(error).__name__, str(error))

class Missing:
    pass

try:
    with Missing():
        0
except Exception as error:
    show(error)

class MissingExit:
    def __enter__(self):
        return self

try:
    with MissingExit():
        0
except Exception as error:
    show(error)

events = []
class EnterLookupFailure:
    def __get__(self, obj, owner=None):
        events.append("enter-lookup")
        raise ValueError("enter lookup failed")

class ExitLookupProbe:
    def __get__(self, obj, owner=None):
        events.append("exit-lookup")
        def exit(exc_type, exc, tb):
            events.append("exit-call")
            return False
        return exit

class LookupManager:
    __enter__ = EnterLookupFailure()
    __exit__ = ExitLookupProbe()

try:
    with LookupManager():
        events.append("body")
except Exception as error:
    show(error)
print(events)

events = []
class EnterCallFailure:
    def __enter__(self):
        events.append("enter-call")
        raise ValueError("enter call failed")

    def __exit__(self, exc_type, exc, tb):
        events.append("exit-call")
        return False

try:
    with EnterCallFailure():
        events.append("body")
except Exception as error:
    show(error)
print(events)

events = []
class ExitCallFailure:
    def __enter__(self):
        events.append("enter-call")
        return self

    def __exit__(self, exc_type, exc, tb):
        events.append("exit-call")
        raise ValueError("exit call failed")

try:
    with ExitCallFailure():
        events.append("body")
except Exception as error:
    show(error)
print(events)

events = []
class ExceptionalExit:
    def __init__(self, suppress=False, replace=False):
        self.suppress = suppress
        self.replace = replace

    def __enter__(self):
        events.append("exception-enter")
        return self

    def __exit__(self, exc_type, exc, tb):
        events.append(exc_type.__name__)
        events.append(str(exc))
        events.append(tb is not None)
        if self.replace:
            raise RuntimeError("exit replaced body")
        return self.suppress

try:
    with ExceptionalExit():
        raise ValueError("body failed")
except ValueError as error:
    events.append("caught:" + str(error))

with ExceptionalExit(suppress=True):
    raise RuntimeError("hidden")
events.append("suppressed")

try:
    with ExceptionalExit(replace=True):
        raise ValueError("original body")
except RuntimeError as error:
    events.append(str(error))
    events.append(type(error.__context__).__name__)
print(events)
