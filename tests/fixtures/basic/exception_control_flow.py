def return_through_finally():
    try:
        return 7
    finally:
        print("return cleanup")


def finally_overrides_return():
    try:
        return 1
    finally:
        return 2


i = 0
while i < 4:
    i = i + 1
    try:
        if i == 2:
            continue
        if i == 3:
            break
        print(i)
    finally:
        print("loop cleanup")

print(return_through_finally())
print(finally_overrides_return())
