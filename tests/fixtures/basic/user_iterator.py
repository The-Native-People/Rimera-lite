count = 0


class Counter:
    def __iter__(self):
        return self

    def __next__(self):
        global count
        if count == 3:
            raise StopIteration
        count = count + 1
        return count


total = 0
for value in Counter():
    total = total + value

print(total)
