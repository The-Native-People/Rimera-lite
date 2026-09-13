def report(label, thunk):
    try:
        print(label, thunk())
    except Exception as error:
        print(label, type(error).__name__, str(error))
g = {'seed': 3}
l = {}
exec('global seed\nanswer = seed + 4\nseed = 8', g, l)
print('writes', g['seed'], l['answer'], 'answer' in g)
exec('del answer\nglobal seed\ndel seed', g, l)
print('deletes', 'answer' in l, 'seed' in g)
report('missing-delete', lambda: exec('del absent', {}, {}))
g = {'value': 10}
l = {'value': 20}
exec('def read(): return value\nclass Item:\n    number = value\n    def read(self): return value', g, l)
print('definitions', l['read'](), l['Item'].number, l['Item']().read())
g['value'] = 30
print('live-globals', l['read']())
class Item:
    base = 7
    exec('value = base + 1')
    answer = eval('value + 1')
print('class-body', Item.value, Item.answer)
events = []
class Mapping:
    def __init__(self):
        self.data = {}
    def __getitem__(self, key):
        events.append(('get', key))
        return self.data[key]
    def __setitem__(self, key, value):
        events.append(('set', key))
        if key == 'bad':
            raise ValueError('write failed')
        self.data[key] = value
    def __delitem__(self, key):
        events.append(('del', key))
        del self.data[key]
m = Mapping()
report('partial', lambda: exec('first = 1\nbad = 2\nlast = 3', {}, m))
print('partial-state', m.data, events)
events.clear()
exec('first += 4\ndel first', {}, m)
print('protocol', events)
def outer():
    first = 1
    second = 2
    def change():
        nonlocal first, second
        first += 10
        second += 20
    return change
change = outer()
print('closure-return', exec(change.__code__, {}, {}, closure=change.__closure__))
print('closure-values', change.__closure__[0].cell_contents, change.__closure__[1].cell_contents)
report('closure-length', lambda: exec(change.__code__, {}, {}, closure=()))
report('closure-cells', lambda: exec(change.__code__, {}, {}, closure=(1, 2)))
report('source-closure', lambda: exec('pass', {}, {}, closure=()))
report('no-free-closure', lambda: exec(compile('1', '<code>', 'exec'), {}, {}, closure=()))
g = {'__builtins__': {'answer': lambda: 11}}
exec('def read(): return answer()', g)
read = g['read']
g['__builtins__'] = {'answer': lambda: 22}
print('captured-builtins', read(), eval('answer()', g))
print('function-namespaces', read.__globals__ is g, read.__builtins__ is g['__builtins__'])
try:
    exec('nonlocal outside', {}, {})
except SyntaxError:
    print('nonlocal rejected')
class Namespace(dict):
    def __setitem__(self, name, value):
        print('dict-set', name)
        super().__setitem__(name, value)
    def __delitem__(self, name):
        print('dict-del', name)
        super().__delitem__(name)
g = Namespace()
exec('local = 1\nglobal direct\ndirect = 2\ndel local\ndel direct', g)
print('dict-writes', 'local' in g, 'direct' in g)
events.clear()
class Meta(type):
    @classmethod
    def __prepare__(cls, name, bases):
        return Mapping()
    def __new__(cls, name, bases, namespace):
        return super().__new__(cls, name, bases, namespace.data)
class Prepared(metaclass=Meta):
    base = 5
    exec('answer = base + 7')
    result = eval('answer + 1')
print('prepared', Prepared.answer, Prepared.result)
print('prepared-write', ('set', 'answer') in events)
g = {'value': 1}
exec('def retained(): return value', g)
retained = g['retained']
for index in range(300):
    temporary = [str(index)] * 30
g['value'] = 42
print('retained', retained())
print('retained-builtins', read())
