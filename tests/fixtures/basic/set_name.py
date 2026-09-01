class Field:
    def __set_name__(self, owner, name):
        self.name = name


class Model:
    field = Field()


print(Model.field.name)
