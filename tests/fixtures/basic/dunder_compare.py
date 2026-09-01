class Left:
    def __eq__(self, other):
        return NotImplemented

    def __lt__(self, other):
        return NotImplemented


class Right:
    def __eq__(self, other):
        return True

    def __gt__(self, other):
        return True


print(Left() == Right())
print(Left() < Right())
