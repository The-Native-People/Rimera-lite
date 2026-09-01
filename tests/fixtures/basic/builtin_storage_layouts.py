class Frozen(tuple):
    pass

class Mapping(dict):
    pass

class Tags(set):
    pass

frozen = Frozen((3, 4))
mapping = Mapping({"name": "rimera"})
tags = Tags((1, 2, 1))
print(type(frozen).__name__, len(frozen), frozen[1])
print(type(mapping).__name__, len(mapping), mapping["name"])
print(type(tags).__name__, len(tags), 2 in tags)
