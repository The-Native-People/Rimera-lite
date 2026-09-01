class Base:
    pass


class Proxy:
    def __mro_entries__(self, bases):
        print("resolve bases")
        return (Base,)


proxy = Proxy()


class Derived(proxy):
    pass


print(issubclass(Derived, Base))
print(Derived.__orig_bases__ == (proxy,))
