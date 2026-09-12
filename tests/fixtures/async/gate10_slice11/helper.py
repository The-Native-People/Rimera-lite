class Descriptor:
    def __get__(self, instance, owner):
        if instance is None:
            return self
        return instance.base + 3


class Box:
    marker = Descriptor()

    def __init__(self, base):
        self.base = base


class ReadyAwait:
    def __init__(self, value):
        self.value = value

    def __await__(self):
        if False:
            yield None
        return self.value


class AsyncRange:
    def __init__(self, limit):
        self.index = 0
        self.limit = limit

    def __aiter__(self):
        return self

    async def __anext__(self):
        if self.index >= self.limit:
            raise StopAsyncIteration
        value = self.index
        self.index += 1
        return value


class Manager:
    def __init__(self, events, name):
        self.events = events
        self.name = name

    async def __aenter__(self):
        self.events.append(self.name + ":enter")
        return self.name

    async def __aexit__(self, exc_type, exc, tb):
        type_name = "None"
        if exc_type is not None:
            type_name = exc_type.__name__
        self.events.append(self.name + ":exit:" + type_name)
        return False


def sync_values(limit):
    index = 0
    while index < limit:
        yield index
        index += 1


def apply(callback, value):
    return callback(value)


async def leaf(value):
    return value + 1


async def chain(depth, value):
    if depth == 0:
        return value
    return await chain(depth - 1, value + 1)


async def stream(limit):
    index = 0
    while index < limit:
        yield index
        index += 1
