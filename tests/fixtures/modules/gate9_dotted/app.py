import pkg.child
print(pkg.child.VALUE)
import pkg.child as pkg
print(pkg.VALUE)

def local():
    import pkg.child as child, pkg as root
    return child is root.child

class Holder:
    import pkg.child as child

def values():
    import pkg.child as child
    yield child.VALUE

print(local(), Holder.child.VALUE, list(values()))

if True:
    import pkg.child as branch
for item in range(2):
    import pkg.child as loop
try:
    raise ValueError("handled")
except ValueError:
    import pkg.child as handler

class Manager:
    def __enter__(self):
        return self

    def __exit__(self, kind, value, traceback):
        return False

with Manager():
    import pkg.child as managed

print(branch is loop, loop is handler, handler is managed)
