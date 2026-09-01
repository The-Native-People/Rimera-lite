class Numbers:
    values = 0
    for value in range(6):
        if value == 2:
            continue
        if value == 5:
            break
        values += value


print(Numbers.values)
