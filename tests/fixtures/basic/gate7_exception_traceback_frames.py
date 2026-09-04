saved = None


def inner(value):
    local = value + 1
    raise ValueError("boom")


def outer():
    marker = "outer"
    try:
        inner(4)
    except ValueError as error:
        global saved
        saved = error
        first = error.__traceback__
        second = first.tb_next
        print(type(first).__name__, type(first.tb_frame).__name__)
        print(first.tb_lineno, second.tb_lineno)
        print(first.tb_frame.f_code.co_name, second.tb_frame.f_code.co_name)
        print(first.tb_frame.f_globals is globals(), second.tb_frame.f_globals is globals())
        print(first.tb_frame.f_locals["marker"], second.tb_frame.f_locals["value"])
        print(second.tb_frame.f_locals["local"])
        print(second.tb_frame.f_back is first.tb_frame)
        print(first.tb_frame is error.__traceback__.tb_frame)
        print(second.tb_frame.f_lineno == second.tb_lineno)

        error.args = ["changed", 7]
        print(error.args)
        error.__traceback__ = None
        print(error.__traceback__ is None)
        error.__traceback__ = first
        print(error.__traceback__ is first)

        cause = RuntimeError("cause")
        context = ValueError("context")
        error.__cause__ = cause
        error.__context__ = context
        error.__suppress_context__ = False
        print(error.__cause__ is cause, error.__context__ is context, error.__suppress_context__)

        first.tb_next = None
        print(first.tb_next is None)
        first.tb_next = second
        print(first.tb_next is second)

        for action in ["traceback", "cause", "context", "suppress", "tb-frame", "tb-line"]:
            try:
                if action == "traceback":
                    error.__traceback__ = 1
                elif action == "cause":
                    error.__cause__ = 1
                elif action == "context":
                    error.__context__ = 1
                elif action == "suppress":
                    error.__suppress_context__ = 1
                elif action == "tb-frame":
                    first.tb_frame = None
                else:
                    first.tb_lineno = 1
            except (TypeError, AttributeError) as failure:
                print(action, type(failure).__name__)


outer()
retained = saved.__traceback__
print(retained.tb_frame.f_code.co_name, retained.tb_next.tb_frame.f_code.co_name)
print(retained.tb_frame.f_locals["marker"], retained.tb_next.tb_frame.f_locals["local"])
print(retained.tb_next.tb_frame.f_back is retained.tb_frame)


def reraised():
    try:
        inner(8)
    except ValueError:
        raise


try:
    reraised()
except ValueError as error:
    first = error.__traceback__
    second = first.tb_next
    third = second.tb_next
    print(first.tb_frame.f_code.co_name, second.tb_frame.f_code.co_name, third.tb_frame.f_code.co_name)


group = ExceptionGroup("group", (ValueError("a"), TypeError("b")))
print(group.args[0], len(group.args[1]))

