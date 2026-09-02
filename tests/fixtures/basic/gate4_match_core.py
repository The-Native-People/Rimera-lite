events = []


def mark(label, value):
    events.append(label)
    return value


class Tokens:
    one = 1
    two = 2


def subject():
    events.append("subject")
    return 2


match subject():
    case Tokens.one:
        print("one")
    case Tokens.two as chosen if mark("guard", True):
        print("two", chosen)
    case _:
        print("fallback")

print(events)

match 7:
    case captured if mark("guard-false", False):
        print("unreachable")
    case 7:
        print("literal-after-guard")
print("captured-after-guard", captured)

match None:
    case None:
        print("none")
    case _:
        print("bad-none")

match True:
    case False:
        print("bad-bool")
    case True:
        print("true")

match 3:
    case 1 | 2:
        print("bad-or")
    case (3 | 4) as choice:
        print("or-as", choice)
    case _:
        print("bad-or-fallback")

match 2:
    case (1 as same) | (2 as same):
        print("or-capture", same)

match "anything":
    case wildcard_name:
        print("capture", wildcard_name)

_ = "kept"
match 999:
    case _:
        print("wildcard")
print("underscore", _)


def local_case(value):
    match value:
        case 10 as local:
            return local
        case other:
            return other + 1


print("local", local_case(10), local_case(20))


class Boom:
    def __eq__(self, other):
        events.append("eq")
        raise ValueError("match equality boom")


class Values:
    target = object()


try:
    match Boom():
        case Values.target:
            print("bad-equality")
except ValueError:
    print("equality-error")


def guard_boom(value):
    events.append("guard-boom")
    raise ValueError("guard boom")


try:
    match 5:
        case guarded if guard_boom(guarded):
            print("bad-guard")
except ValueError:
    print("guard-error")
print("guarded-after-error", guarded)

print(events)
