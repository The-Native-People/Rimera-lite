try:
    raise ExceptionGroup(
        "group",
        (ValueError("value"), TypeError("type")),
    )
except* ValueError:
    print("value subgroup")
except* TypeError:
    print("type subgroup")
finally:
    print("group cleanup")

print("done")
