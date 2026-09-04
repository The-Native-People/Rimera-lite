class Provider:
    def __init__(self, data, events):
        self.data = bytearray(data)
        self.events = events

    def __buffer__(self, flags):
        self.events.append(("get", flags))
        self.inner = memoryview(self.data)
        return self.inner

    def __release_buffer__(self, view):
        self.events.append(("release", view.tobytes()))


def churn(limit):
    for index in range(limit):
        text = str(index) + "-buffer-pressure-" + str(index)
        pair = (text, index)
        [pair, text, index]


events = []
provider = Provider(b"abcdef", events)
view = memoryview(provider)
child = view[1:5]
churn(700)
print(view.tobytes(), child.tobytes(), events)
view.release()
churn(700)
print(child.tobytes(), events)
child[0] = 90
print(provider.data)
child.release()
print(events)
