class KeywordMeta(type):
    @classmethod
    def __prepare__(mcls, name, bases, **keywords):
        print(keywords["mode"])
        return {}

    def __new__(mcls, name, bases, namespace, **keywords):
        print(keywords["mode"])
        return type(name, bases, namespace)


class KeywordClass(metaclass=KeywordMeta, mode="native"):
    pass


print(type(KeywordClass) == KeywordMeta)
