data = {"first": 1, "second": 2, "first": 3}
values = {1, 2, 3, 3}
print(data)
print(values)
print(len(data), len(values))
print(data["first"], "second" in data, 3 in values, 4 not in values)
data["third"] = 4
for key in data:
    print(key)
for value in values:
    print(value)
