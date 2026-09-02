events = []

class Seq(list):
    def __len__(self):
        junk = None
        for ignored in range(80):
            junk = "ssssssssssssssssssssssssssssssssssssssssssssssssssssssssssssssssssssssssssssssssssssssssssssssssssssssssssssssssssssssssssssssss"
        events.append("seq-len")
        return 4

class Mapping(dict):
    def __len__(self):
        junk = None
        for ignored in range(80):
            junk = "llllllllllllllllllllllllllllllllllllllllllllllllllllllllllllllllllllllllllllllllllllllllllllllllllllllllllllllllllllllllllllllll"
        events.append("map-len")
        return 3

    def get(self, key, default=None):
        junk = None
        for ignored in range(80):
            junk = "gggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggggg"
        events.append("get:" + key)
        if key == "seq":
            return Seq([1, 2, 3, 4])
        if key == "keep":
            return "alive"
        if key == "extra":
            return 9
        return default

subject = Mapping({"seq": Seq([1, 2, 3, 4]), "keep": "alive", "extra": 9})
previous = "original"
match subject:
    case {"seq": [first, *middle, last], "keep": previous, **rest}:
        print("match", first, middle, last, previous, rest)
    case _:
        print("bad")

print(events)
