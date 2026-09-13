namespace = {'callback': lambda value: value + 1}
exec("import inspect\nmodule_name = inspect.__name__\ncallback_result = callback(41)", namespace)
print('import-callback', namespace['module_name'], namespace['callback_result'])

exec("nested = eval('6 * 7')", namespace)
print('nested', namespace['nested'])

exec("def values():\n    yield 20\n    yield 22", namespace)
print('generator', list(namespace['values']()))

exec("async def ready():\n    return 42", namespace)
coroutine = namespace['ready']()
try:
    coroutine.send(None)
except StopIteration as stopped:
    print('coroutine', stopped.value)

exec("class Descriptor:\n    def __get__(self, instance, owner):\n        return 42\nclass Item:\n    answer = Descriptor()", namespace)
print('descriptor', namespace['Item']().answer)

failure = compile("def fail():\n    raise ValueError('dynamic boom')", '<dynamic-trace>', 'exec')
exec(failure, namespace)
try:
    namespace['fail']()
except ValueError as error:
    traceback = error.__traceback__
    print('exception', type(error).__name__, str(error))
    print('trace', traceback.tb_frame.f_code.co_filename, traceback.tb_lineno)

def invoke(callback, value):
    return callback(value)

namespace['invoke'] = invoke
exec("composed = invoke(lambda value: value * 2, 21)", namespace)
print('reverse-callback', namespace['composed'])

def replaced():
    return 0

exec("def replacement():\n    return 42", namespace)
replaced.__code__ = namespace['replacement'].__code__
del namespace['replacement']
for index in range(300):
    transient = [str(index)] * 20
print('code-replacement', replaced())
