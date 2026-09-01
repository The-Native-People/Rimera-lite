class Base:
    pass


class Invalid(type("Temporary", (Base,), {})):
    pass
