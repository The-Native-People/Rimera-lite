def make(seed):
    held = [seed]
    value = seed + 1
    yield held
    yield value


for index in range(160):
    dead = make(index)
    next(dead)
    dead.close()
    dead = None
    pressure = [index, index + 1, index + 2]

generator = make(999)
frame = generator.gi_frame
print(frame.f_locals["seed"])
print(next(generator)[0])
locals_view = frame.f_locals
print(locals_view["seed"], locals_view["value"], locals_view["held"][0])
print(generator.gi_suspended, frame is generator.gi_frame)
generator.close()
print(generator.gi_frame is None, generator.gi_suspended)
generator = None
for index in range(200):
    pressure = [index, index + 1, index + 2, index + 3]
print(frame.f_code.co_name, frame.f_locals["seed"], frame.f_locals["held"][0])
print(locals_view is frame.f_locals)


delegated = (value for value in [1, 2, 3])
retained = delegated.gi_frame
print(next(delegated), retained.f_code.co_name)
delegated.close()
print(delegated.gi_frame is None, retained.f_locals is retained.f_locals)
