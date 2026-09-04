def reveal[T](value):
    return T


class Holder[T]:
    token = T


type Alias[T] = T

function_param = reveal.__type_params__[0]
class_param = Holder.__type_params__[0]
alias_param = Alias.__type_params__[0]


def churn(limit):
    for index in range(limit):
        text = str(index) + "-reflection-pressure-" + str(index)
        pair = (text, index)
        [pair, text, index]


churn(700)
print(reveal(1) is function_param)
print(Holder.token is class_param)
print(Alias.__value__ is alias_param)
print(function_param.__name__, class_param.__name__, alias_param.__name__)
print(tuple(type(value).__name__ for value in (function_param, class_param, alias_param)))
