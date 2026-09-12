events = []


class Truth:
    def __init__(self, name, value=None, error=None):
        self.name = name
        self.value = value
        self.error = error

    def __bool__(self):
        events.append("truth:" + self.name)
        if self.error is not None:
            raise self.error
        return self.value


class Manager:
    def __init__(self, name, result=False, error=None, reraise=False):
        self.name = name
        self.result = result
        self.error = error
        self.reraise = reraise

    def __enter__(self):
        events.append("enter:" + self.name)
        return self

    def __exit__(self, exc_type, exc, traceback):
        events.append(("exit", self.name, exc_type.__name__))
        events.append(("triple", exc_type is type(exc), traceback is exc.__traceback__))
        if self.reraise:
            raise
        if self.error is not None:
            raise self.error
        return self.result


original = ValueError("body")
try:
    with Manager("plain"):
        raise original
except ValueError as error:
    events.append(("caught", error is original, error.__traceback__.tb_lineno))
print(events)

events.clear()
with Manager("suppress", Truth("yes", True)):
    raise TypeError("hidden")
events.append("continued")
print(events)

events.clear()
try:
    with Manager("false", Truth("no", False)):
        raise TypeError("visible")
except TypeError as error:
    events.append(("caught", str(error)))
print(events)

events.clear()
body_error = ValueError("before-truth")
try:
    with Manager("truth-error", Truth("boom", error=RuntimeError("truth failed"))):
        raise body_error
except RuntimeError as error:
    events.append(("replacement", str(error), error.__context__ is body_error))
print(events)

events.clear()
body_error = ValueError("before-exit")
try:
    with Manager("exit-error", error=RuntimeError("exit failed")):
        raise body_error
except RuntimeError as error:
    events.append(("replacement", str(error), error.__context__ is body_error))
print(events)

events.clear()
original = ValueError("reraised")
try:
    with Manager("reraise", reraise=True):
        raise original
except ValueError as error:
    events.append(("same", error is original))
print(events)

events.clear()
body_error = ValueError("inner-body")
inner_error = RuntimeError("inner-exit")
try:
    with Manager("outer", Truth("outer-suppress", True)), Manager(
        "inner", error=inner_error
    ):
        raise body_error
except BaseException as error:
    events.append(("unexpected", type(error).__name__))
events.append(
    (
        "chain",
        inner_error.__context__ is body_error,
        inner_error.__cause__ is None,
        inner_error.__suppress_context__,
    )
)
print(events)

events.clear()
# Exception groups must pass through the same exact exceptional-exit triple.
group = ExceptionGroup("group", (ValueError("left"), TypeError("right")))
try:
    with Manager("group"):
        raise group
except ExceptionGroup as error:
    events.append(("group-same", error is group))
print(events)

events.clear()
try:
    with Manager("memory", error=MemoryError("cleanup allocation")):
        raise ValueError("pending")
except MemoryError as error:
    events.append(("memory", str(error), type(error.__context__).__name__))
print(events)
