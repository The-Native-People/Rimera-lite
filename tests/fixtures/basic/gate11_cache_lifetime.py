def pressure():
    for index in range(450):
        transient = [str(index)] * 24


first = compile('def make(value):\n    return lambda: value + 1', '<shared>', 'exec')
second = compile('def make(value):\n    return lambda: value + 1', '<shared>', 'exec')
print('distinct-code', first is second)

namespace = {}
exec(first, namespace)
escaped = namespace['make'](41)
del first
del second
del namespace
pressure()
print('escaped', escaped())

expression = compile('20 + 22', '<mode>', 'eval')
statement = compile('answer = 42', '<mode>', 'exec')
print('key-mode', eval(expression))
target = {}
exec(statement, target)
print('key-exec', target['answer'])

optimized = compile('__debug__', '<options>', 'eval', optimize=1)
ordinary = compile('__debug__', '<options>', 'eval', optimize=0)
print('key-options', eval(ordinary), eval(optimized))

for cycle in range(12):
    code = compile('value + 1', '<reclaim>', 'eval')
    print('cycle', cycle, eval(code, {'value': cycle}))
    del code
    pressure()
