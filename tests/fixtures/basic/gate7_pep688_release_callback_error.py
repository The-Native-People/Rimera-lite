class ReleaseBoom:
    def __init__(self):
        self.data = bytearray(b"zz")
        self.events = []

    def __buffer__(self, flags):
        return memoryview(self.data)

    def __release_buffer__(self, view):
        self.events.append("release")
        raise RuntimeError("boom")


provider = ReleaseBoom()
view = memoryview(provider)
view.release()
print("release returned")
print(provider.events)
