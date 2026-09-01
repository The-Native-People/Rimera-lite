class LoopCleanup:
    value = 0
    while True:
        try:
            break
        finally:
            value = 1

print(LoopCleanup.value)
