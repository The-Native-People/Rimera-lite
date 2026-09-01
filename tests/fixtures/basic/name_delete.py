value = 41
del value

try:
    print(value)
except NameError:
    print("gone")


def local_delete():
    value = 7
    del value
    try:
        print(value)
    except UnboundLocalError:
        print("local gone")


local_delete()
