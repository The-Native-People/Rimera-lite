try:
    raise ExceptionGroup(
        "group",
        (ValueError("value"), TypeError("type")),
    )
except* ValueError:
    print("handled value")
