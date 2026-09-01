class Mapping:
    def __init__(self, entries, log):
        self.entries = entries
        self.log = log

    def keys(self):
        self.log.append(("keys", len(self.entries)))
        return [entry[0] for entry in self.entries]

    def __getitem__(self, key):
        self.log.append(("get", key))
        for current, value in self.entries:
            if current == key:
                return value
        raise KeyError(key)


log = []
source = Mapping([("b", 20), ("c", 30)], log)
value = {"a": 1, **source, "b": 2}
print(value)
print(log)

print({1: "one", **{2: "two"}, 1: "again"})

class Key:
    def __init__(self, label, log):
        self.label = label
        self.log = log

    def __hash__(self):
        self.log.append(("hash", self.label))
        return 7

    def __eq__(self, other):
        self.log.append(("eq", self.label, other.label))
        return self.label == other.label

    def __repr__(self):
        return "Key(" + self.label + ")"


collision_log = []
first = Key("same", collision_log)
second = Key("same", collision_log)
result = {first: 1, **{second: 2}}
print(result)
print(collision_log)

failure_log = []
class Broken:
    def keys(self):
        failure_log.append("keys")
        return ["ok", "boom"]

    def __getitem__(self, key):
        failure_log.append(("get", key))
        if key == "boom":
            raise ValueError("mapping exploded")
        return 9

try:
    failed = {"before": 1, **Broken(), "after": 2}
except ValueError as error:
    print(type(error).__name__, str(error), failure_log)
