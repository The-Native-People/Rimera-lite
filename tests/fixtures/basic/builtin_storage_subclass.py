class Names(list):
    pass

value = Names([1, 2])
print(type(value).__name__)
print(isinstance(value, list))
print(len(value))
print(value[0])
value[1] = 9
print(value[1])
