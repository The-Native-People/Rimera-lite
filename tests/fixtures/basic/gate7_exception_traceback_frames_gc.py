def capture(seed):
    marker = [seed]
    try:
        raise ValueError(seed)
    except ValueError as error:
        return error.__traceback__, marker


for index in range(160):
    dead_traceback, dead_marker = capture(index)
    dead_traceback = None
    dead_marker = None
    pressure = [index, index + 1, index + 2]

traceback, marker = capture(999)
frame = traceback.tb_frame
locals_view = frame.f_locals
for index in range(160):
    pressure = [index, index + 1, index + 2, index + 3]

print(frame is traceback.tb_frame)
print(frame.f_code.co_name, frame.f_locals["seed"])
print(frame.f_locals["marker"] is marker, marker[0])
print(locals_view is frame.f_locals)
traceback = None
marker = None
for index in range(160):
    pressure = [index, index + 1, index + 2]
print(frame.f_locals["seed"], frame.f_locals["marker"][0])

