global_value = 1


def set_global():
    global global_value
    global_value = 9


def local_error():
    try:
        print(local_value)
    except UnboundLocalError as error:
        print(error)
    local_value = 1


def free_error():
    def read_captured():
        return captured

    try:
        read_captured()
    except NameError as error:
        print(error)
    captured = 1


def handler_binding_is_cleared():
    try:
        raise ValueError("temporary")
    except ValueError as temporary:
        print(temporary)
    try:
        print(temporary)
    except UnboundLocalError as error:
        print(error)


print(global_value)
set_global()
print(global_value)
local_error()
free_error()
handler_binding_is_cleared()

try:
    print(missing_global)
except NameError as error:
    print(error)
