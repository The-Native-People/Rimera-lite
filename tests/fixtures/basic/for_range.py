total = 0
for value in range(1, 8, 2):
    total = total + value
print(total)

for character in "go":
    print(character)

for value in [1, 2, 3]:
    if value == 2:
        continue
    print(value)
else:
    print("done")
