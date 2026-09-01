emit = print
count = len
make_int = int
make_list = list
make_range = range
make_chr = chr
read_ord = ord
sort_values = sorted
repr_value = repr
type_value = type

aliases = [
    abs,
    all,
    any,
    ascii,
    bin,
    bool,
    bytearray,
    bytes,
    callable,
    chr,
    classmethod,
    complex,
    delattr,
    dict,
    divmod,
    enumerate,
    filter,
    float,
    format,
    frozenset,
    getattr,
    hasattr,
    hash,
    hex,
    id,
    int,
    isinstance,
    issubclass,
    iter,
    len,
    list,
    map,
    max,
    memoryview,
    min,
    next,
    object,
    oct,
    ord,
    pow,
    print,
    property,
    range,
    repr,
    reversed,
    round,
    set,
    setattr,
    slice,
    sorted,
    staticmethod,
    str,
    sum,
    super,
    tuple,
    type,
    zip,
]
emit("count", count(aliases))
emit("constants", Ellipsis, repr_value(Ellipsis), type_value(Ellipsis), __debug__, Ellipsis is Ellipsis, NotImplemented is NotImplemented)

abs = 1
all = 2
any = 3
ascii = 4
bin = 5
bool = 6
bytearray = 7
bytes = 8
callable = 9
chr = 10
classmethod = 11
complex = 12
delattr = 13
dict = 14
divmod = 15
enumerate = 16
filter = 17
float = 18
format = 19
frozenset = 20
getattr = 21
hasattr = 22
hash = 23
hex = 24
id = 25
int = 26
isinstance = 27
issubclass = 28
iter = 29
len = 30
list = 31
map = 32
max = 33
memoryview = 34
min = 35
next = 36
object = 37
oct = 38
ord = 39
pow = 40
print = 41
property = 42
range = 43
repr = 44
reversed = 45
round = 46
set = 47
setattr = 48
slice = 49
sorted = 50
staticmethod = 51
str = 52
sum = 53
super = 54
tuple = 55
type = 56
zip = 57
Ellipsis = 58
NotImplemented = 59

shadowed = [
    abs,
    all,
    any,
    ascii,
    bin,
    bool,
    bytearray,
    bytes,
    callable,
    chr,
    classmethod,
    complex,
    delattr,
    dict,
    divmod,
    enumerate,
    filter,
    float,
    format,
    frozenset,
    getattr,
    hasattr,
    hash,
    hex,
    id,
    int,
    isinstance,
    issubclass,
    iter,
    len,
    list,
    map,
    max,
    memoryview,
    min,
    next,
    object,
    oct,
    ord,
    pow,
    print,
    property,
    range,
    repr,
    reversed,
    round,
    set,
    setattr,
    slice,
    sorted,
    staticmethod,
    str,
    sum,
    super,
    tuple,
    type,
    zip,
]
expected = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31, 32, 33, 34, 35, 36, 37, 38, 39, 40, 41, 42, 43, 44, 45, 46, 47, 48, 49, 50, 51, 52, 53, 54, 55, 56, 57]
emit("shadowed", shadowed == expected, Ellipsis, NotImplemented)
emit("aliases", make_int("12"), make_list((1, 2)), make_list(make_range(3)), read_ord(make_chr(65)), sort_values([3, 1, 2]))
