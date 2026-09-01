def mark():
    print("mark")
    return 7


print(0 and mark())
print(1 or mark())
print(0 or mark())
print("left" and "right")
print("left" or "right")
