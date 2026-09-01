def choose(flag):
    try:
        if flag:
            raise ValueError("boom")
    except (ValueError, TypeError) as error:
        print("caught")
    else:
        print("clean")
    finally:
        print("finally")
    return 7


def reraised():
    try:
        try:
            raise ValueError("again")
        except ValueError:
            raise
    except ValueError:
        print("caught again")


print(choose(True))
print(choose(False))
reraised()
