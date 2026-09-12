code = compile('left + right', 'expression.py', 'eval')
print(code.co_name, code.co_filename, code.co_firstlineno, code.co_flags)
print(eval(code, {'left': 20}, {'right': 22}))
g = {'seed': 7}
l = {}
exec('value = seed + 1\ndef read():\n    return seed\n', g, l)
print(l['value'], l['read'](), 'value' in g, '__builtins__' in g)
exec('global seed\nseed = 10\ndel value', g, l)
print(g['seed'], 'value' in l, l['read']())
print(eval(b'6 * 7', {}))
print(eval(bytearray(b'40 + 2'), {}))
print(eval('len([1, 2])', {}))
try:
    eval('len([])', {'__builtins__': {}})
except NameError:
    print('restricted builtins')
exec('class Item:\n    value = 13\n', g, l)
print(l['Item'].value)
class Mapping:
    def __init__(self):
        self.data = {}
    def __getitem__(self, key):
        return self.data[key]
    def __setitem__(self, key, value):
        self.data[key] = value
    def __delitem__(self, key):
        del self.data[key]
m = Mapping()
exec('answer = seed * 2', g, m)
print(eval('answer + 1', g, m))
exec('del answer', g, m)
print('answer' in m.data)
try:
    compile('x =', 'broken.py', 'exec')
except SyntaxError:
    print('syntax failure')
exec(compile("'hello'", '<single>', 'single'))
exec(compile('None', '<single>', 'single'))
exec(compile('1; 2', '<single>', 'single'))
def implicit():
    number = 41
    print(eval('number + 1'))
implicit()
g = {}
l = {}
exec('answer: int = 42', g, l)
print(l['answer'], l['__annotations__']['answer'] is int, '__annotations__' in g)
print(eval('print(10)', {'print': lambda x: x + 2}))
print(eval(memoryview(b'21 * 2'), {}))
def outer():
    value = 5
    def inner():
        nonlocal value
        value += 3
    return inner
function = outer()
exec(function.__code__, {}, {}, closure=function.__closure__)
print(function.__closure__[0].cell_contents)
try:
    eval(function.__code__)
except TypeError:
    print('eval rejects closure')
try:
    exec(function.__code__, {}, {}, closure=())
except TypeError:
    print('exec validates closure')
try:
    eval('1', {}, 42)
except TypeError:
    print('locals validated')
try:
    compile('value =\n', 'bad.py', 'exec')
except SyntaxError as error:
    print(error.filename, error.lineno, error.text == 'value =\n')
namespace = {}
code = compile('counter = counter + 1', 'reuse.py', 'exec')
namespace['counter'] = 0
for index in range(30):
    exec(code, namespace)
print(namespace['counter'])
