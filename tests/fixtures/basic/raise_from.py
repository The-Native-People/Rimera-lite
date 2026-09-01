try:
    ignored = 1 // 0
except ZeroDivisionError:
    raise RuntimeError("wrapped") from ValueError("cause")
