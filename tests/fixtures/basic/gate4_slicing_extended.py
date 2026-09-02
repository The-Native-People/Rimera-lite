index_events = []


class Index:
    def __init__(self, label, value):
        self.label = label
        self.value = value

    def __index__(self):
        index_events.append(self.label)
        return self.value

    def __repr__(self):
        return "Index(" + self.label + ")"


values = [0, 1, 2, 3, 4, 5]
print(values[Index("single", -1)])
print(values[Index("start", 1):Index("stop", 6):Index("step", 2)])
print(index_events)
print("abcdef"[1:6:2])
print(b"abcdef"[::-2])
print((0, 1, 2, 3, 4)[4:0:-2])
print(list(range(10)[1:9:3]))

items = [0, 1, 2, 3, 4, 5]
items[1:5:2] = [10, 30]
print(items)
del items[::2]
print(items)

raw = bytearray(b"abcdef")
raw[1:5:2] = b"XY"
print(bytes(raw))
del raw[1:4]
print(bytes(raw))


class Recorder:
    def __init__(self):
        self.log = []

    def __getitem__(self, key):
        self.log.append(("get", key))
        return key

    def __setitem__(self, key, value):
        self.log.append(("set", key, value))

    def __delitem__(self, key):
        self.log.append(("del", key))


recorder = Recorder()
print(recorder[1:5:2, 3])
recorder[1:4, 2] = "value"
del recorder[:, 1]
print(recorder.log)

backing = bytearray(b"abcdef")
grid = memoryview(backing).cast("B", shape=[2, 3])
print(grid[1, 2])
grid[0, 1] = 90
print(bytes(backing))

try:
    print(values[::0])
except ValueError as error:
    print(type(error).__name__, str(error))
