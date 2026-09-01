class Finalized:
    try:
        raise ValueError("boom")
    except ValueError:
        value = 1
    finally:
        value += 1

print(Finalized.value)
