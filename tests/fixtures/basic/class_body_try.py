class Captured:
    try:
        raise ValueError("boom")
    except ValueError as error:
        message = str(error)
    else:
        message = "wrong"

print(Captured.message)
print(hasattr(Captured, "error"))
