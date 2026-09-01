def invalid():
    try:
        raise ExceptionGroup("group", (ValueError("value"),))
    except* ValueError:
        return 1
