class Thing:
    __match_args__ = ("value",)

    def __init__(self, value):
        self.value = value

match Thing(3):
    case Thing(value):
        print(value)
