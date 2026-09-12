try:
    from partial_all import *
except AttributeError as error:
    print(type(error).__name__)

try:
    from invalid_all import *
except TypeError as error:
    print(type(error).__name__)

print(FIRST, OK)
