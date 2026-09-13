def report(label, thunk):
    try:
        print(label, thunk())
    except Exception as error:
        print(label, type(error).__name__, str(error))

value = 10
print('precedence', eval('value'), eval('value', {'value': 20}), eval('value', {'value': 20}, {'value': 30}))
print('none', eval('value', None, {'value': 40}), eval('value', {'value': 50}, None))
print('whitespace', eval(' \t 6 * 7 \t '))
print('code', eval(compile('value + 1', 'eval.py', 'eval'), {'value': 41}))
print('exec-code', eval(compile('answer = 42', 'exec.py', 'exec'), {}))

def local_scope():
    local = 7
    print('local', eval('local'), eval('locals() is supplied', None, {'supplied': locals()}))
    exec('created = 9')
    print('snapshot', eval('created'), local)
local_scope()
def enclosing():
    captured = 8
    hidden = 99
    def inner():
        print('capture', captured, eval('captured'))
        report('not-captured', lambda: eval('hidden'))
    return inner
enclosing()()

class Mapping:
    def __getitem__(self, name):
        print('lookup', name)
        if name == 'local':
            return 12
        if name == 'broken':
            raise ValueError('mapping failure')
        raise KeyError(name)
m = Mapping()
print('mapping', eval('local + shared', {'shared': 30}, m))
report('mapping-error', lambda: eval('broken', {}, m))
report('missing', lambda: eval('absent', {}, m))
report('globals-error', lambda: eval('1', m))
report('locals-error', lambda: eval('1', {}, 1))
report('keyword-error', lambda: eval('1', globals={}))
report('arity-error', lambda: eval())

class Namespace(dict):
    def __getitem__(self, name):
        print('dict-get', name)
        return super().__getitem__(name)
g = Namespace(value=42)
print('dict-subclass', eval('value', g))
print('injected', '__builtins__' in g)
for source in ['bad =', 42]:
    g = {}
    try:
        eval(source, g)
    except Exception as error:
        print('failed-injection', type(error).__name__, '__builtins__' in g)
print('custom-builtins', eval('answer()', {'__builtins__': {'answer': lambda: 42}}))
report('empty-builtins', lambda: eval('len([])', {'__builtins__': {}}))
report('none-builtins', lambda: eval('absent', {'__builtins__': None}))
def plain():
    return 42
def variadic(*args, **kwargs):
    return len(args) + len(kwargs)
print('function-code', eval(plain.__code__), eval(variadic.__code__))
def required(value):
    return value
report('required', lambda: eval(required.__code__))
def generator():
    yield 42
print('generator-code', next(eval(generator.__code__)))
print('optimized-string', eval(compile("'retained'", '<eval>', 'eval', optimize=2)))
g = {'value': 12}
print('identity', eval('globals() is locals()', g), eval('globals() is locals()', g, {}))
g = {}
exec(compile('def check():\n    assert (value := 1)\n    return value', '<optimized>', 'exec', optimize=1), g)
report('optimized-binding', g['check'])
class Missing(KeyError):
    pass
class MissingMapping:
    def __getitem__(self, name):
        raise Missing(name)
print('subclass-missing', eval('value', {'value': 42}, MissingMapping()))
failure = ValueError('identity')
class FailingMapping:
    def __getitem__(self, name):
        raise failure
try:
    eval('value', {}, FailingMapping())
except ValueError as caught:
    print('exception-identity', caught is failure)
