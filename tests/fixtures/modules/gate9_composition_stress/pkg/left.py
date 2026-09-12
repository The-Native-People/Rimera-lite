events = []
import pkg.right


class Manager:
    def __enter__(self):
        events.append("enter")
        return self

    def __exit__(self, exc_type, exc, tb):
        events.append("exit")
        return False


def stream():
    with Manager():
        yield "value"


def cycle_value():
    return pkg.right.value
