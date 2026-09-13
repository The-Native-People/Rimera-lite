def report(label, action):
    try:
        action()
    except Exception as error:
        print(label, type(error).__name__, str(error))


oversized = bytearray(1048577)
report('source-limit', lambda: compile(oversized, '<large>', 'exec'))
print('source-recovery', eval('40 + 2'))

recursive_source = 'eval(recursive_source, globals())'
report('depth-limit', lambda: eval(recursive_source, globals()))
print('depth-recovery', eval('6 * 7'))

namespace = {'sentinel': 42}
report('syntax', lambda: exec('if', namespace))
print('syntax-state', namespace['sentinel'], '__builtins__' in namespace, 'if' in namespace)

report('runtime', lambda: exec("first = 1\nraise ValueError('stop')\nlast = 2", namespace))
print('runtime-state', namespace['first'], 'last' in namespace)
