class Missing:
    def __getattr__(self, name):
        return "missing " + name


class Controlled:
    def __getattribute__(self, name):
        return "controlled " + name

    def __setattr__(self, name, value):
        print("set", name, value)

    def __delattr__(self, name):
        print("delete", name)


print(Missing().value)
item = Controlled()
print(item.value)
item.value = 9
del item.value
