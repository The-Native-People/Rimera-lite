def mark(cls):
    cls.state = "decorated"
    return cls


@mark
class Marked:
    pass


print(Marked.state)
