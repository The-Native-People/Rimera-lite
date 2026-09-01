class HandlerExit:
    while True:
        try:
            raise ValueError("boom")
        except ValueError as error:
            break
    value = "after loop"

print(HandlerExit.value)
print(hasattr(HandlerExit, "error"))
