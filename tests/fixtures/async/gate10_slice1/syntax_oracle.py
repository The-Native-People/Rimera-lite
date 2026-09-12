import json


CASES = {
    "valid_async_def": "async def f():\n    return 1\n",
    "valid_async_for": "async def f(xs):\n    async for x in xs:\n        pass\n",
    "valid_async_with": "async def f(cm):\n    async with cm:\n        pass\n",
    "valid_async_comprehension": "async def f(xs):\n    return [x async for x in xs]\n",
    "valid_async_generator": "async def f():\n    yield 1\n",
    "await_at_module": "await value\n",
    "await_in_sync_function": "def f():\n    return await value\n",
    "async_for_at_module": "async for x in xs:\n    pass\n",
    "async_with_at_module": "async with cm:\n    pass\n",
    "async_comprehension_at_module": "result = [x async for x in xs]\n",
    "yield_from_in_async_function": "async def f():\n    yield from xs\n",
    "return_value_in_async_generator": "async def f():\n    yield 1\n    return 2\n",
}


def main():
    report = {}
    for name, source in CASES.items():
        try:
            compile(source, f"<{name}>", "exec")
        except SyntaxError as error:
            report[name] = {
                "status": "SyntaxError",
                "message": error.msg,
                "line": error.lineno,
                "offset": error.offset,
            }
        else:
            report[name] = {"status": "ok"}
    print(json.dumps(report, sort_keys=True, separators=(",", ":")))


if __name__ == "__main__":
    main()
