try:
    raise ExceptionGroup(
        "original",
        (ValueError("value"), TypeError("type")),
    )
except* ValueError:
    raise
