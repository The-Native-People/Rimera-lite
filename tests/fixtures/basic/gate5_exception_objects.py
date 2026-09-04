class CustomError(ValueError):
    def __init__(self, message="default", *, code=1):
        self.args = (message,)
        self.code = code

    def describe(self):
        return self.code, self.args


error = CustomError("boom", code=9)
print(type(error).__name__, isinstance(error, ValueError), isinstance(error, Exception))
print(error.args, str(error), error.code, error.describe())
print(error.__traceback__, error.__cause__, error.__context__, error.__suppress_context__)
error.extra = [1, 2]
print(error.extra, error.__dict__["code"], error.__dict__["extra"])
del error.extra
print(hasattr(error, "extra"))


saved = None
try:
    raise error
except CustomError as caught:
    print(caught is error, caught.args, caught.code)
    saved = caught.__traceback__
    lines = []
    current = saved
    while current is not None:
        lines.append(current.tb_lineno)
        current = current.tb_next
    print(lines)
    try:
        saved.tb_lineno = 1
    except AttributeError as metadata_error:
        print(type(metadata_error).__name__, str(metadata_error))
    print(caught.with_traceback(None) is caught, caught.__traceback__)
    print(caught.with_traceback(saved) is caught, caught.__traceback__ is saved)


class RaisedByType(Exception):
    def __init__(self):
        self.args = ("typed",)
        self.initialized = True


try:
    raise RaisedByType
except RaisedByType as typed:
    print(typed.initialized, typed.args, str(typed))


try:
    raise 123
except TypeError as invalid:
    print(type(invalid).__name__, str(invalid))


try:
    raise ValueError("inner")
except ValueError as inner:
    try:
        raise CustomError("outer", code=20) from inner
    except CustomError as outer:
        print(outer.__cause__ is inner, outer.__context__ is inner, outer.__suppress_context__)
        outer.__cause__ = None
        print(outer.__cause__, outer.__suppress_context__)
        outer.__context__ = None
        outer.__suppress_context__ = False
        print(outer.__context__, outer.__suppress_context__)
        for attribute, value in [
            ("__traceback__", 1),
            ("__cause__", 1),
            ("__context__", 1),
            ("__suppress_context__", 1),
        ]:
            try:
                setattr(outer, attribute, value)
            except TypeError as metadata_error:
                print(attribute, type(metadata_error).__name__, str(metadata_error))


cycle = CustomError("cycle")
cycle.peer = cycle
cycle.__cause__ = cycle
cycle.__context__ = cycle
print(cycle.peer is cycle, cycle.__cause__ is cycle, cycle.__context__ is cycle)
cycle = None
for index in range(100):
    pressure = [index, index + 1, index + 2, index + 3]
print("cycles-reclaimable")
