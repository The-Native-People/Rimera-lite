# Simple CLI app to calculate
# That is pretty fun :D

exit = 1
while exit != 0:
    print("Simple CLI")
    print("Exit: 0 \nAdd: 1")
    print()
    ask = str(input("What do you want to do?"))
    if ask == 0:
        exit = 0
    else:
        num1 = int(input("Number 1 to add"))
        num2 = int(input("Number 2 to add"))
        print(f"{num1 + num2}")