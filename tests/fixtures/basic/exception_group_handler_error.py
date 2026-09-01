try:
    raise ExceptionGroup(
        "group",
        (ValueError("value"), TypeError("type")),
    )
except* ValueError:
    raise RuntimeError("handler failed")
except* TypeError:
    print("type handler still ran")
