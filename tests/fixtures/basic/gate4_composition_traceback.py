class BrokenFormat:
    def __format__(self, spec):
        raise ValueError("composition traceback")


def render():
    return [f"item={value:boom}" for value in [BrokenFormat()]]


render()
