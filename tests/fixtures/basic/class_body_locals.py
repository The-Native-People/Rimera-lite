class Values:
    start = 40
    answer = start + 2
    if answer == 42:
        label = "native"
    else:
        label = "wrong"


print(Values.answer)
print(Values.label)
