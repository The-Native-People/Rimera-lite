async def immediate():
    return 1


def final_state():
    coroutine = None
    index = -1
    for index in range(5):
        coroutine = immediate()
        coroutine.close()
    print("final", index, coroutine.cr_frame is None)


def empty_state():
    marker = "unchanged"
    index = -7
    for index in range(0):
        marker = immediate()
        marker.close()
    print("empty", index, marker)


def one_state():
    coroutine = None
    index = -1
    for index in range(9, 10):
        coroutine = immediate()
        coroutine.close()
    print("one", index, coroutine.cr_frame is None)


final_state()
empty_state()
one_state()
