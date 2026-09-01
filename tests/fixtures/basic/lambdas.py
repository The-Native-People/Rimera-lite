def make(offset):
    return lambda value=2, *, scale=3: offset + value * scale


function = make(10)
print(function())
print(function(4, scale=5))
