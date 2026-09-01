try:
    ignored = 1 // 0
finally:
    raise RuntimeError("finally failed")
