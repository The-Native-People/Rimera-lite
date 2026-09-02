events = []


class Probe:
    def __init__(self, label):
        self.label = label

    def __eq__(self, other):
        junk = None
        for ignored in range(80):
            junk = "cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc"
        events.append("eq")
        return other == 7


class Values:
    miss = 8
    hit = 7


def guard_false(value):
    junk = None
    for ignored in range(80):
        junk = "dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd"
    events.append("guard-false")
    return False


def guard(value):
    junk = None
    for ignored in range(80):
        junk = "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee"
    events.append("guard")
    return value.label == "alive"


tentative = "original"
subject = Probe("alive")
match subject:
    case Values.miss as tentative:
        print("bad-first-case")
    case Values.hit as guard_capture if guard_false(guard_capture):
        print("bad-guard-body")
    case Values.hit as captured if guard(captured):
        print("matched", captured.label)
    case _:
        print("bad-fallback")

print("tentative", tentative)
print("guard-capture", guard_capture.label)
print(events)
