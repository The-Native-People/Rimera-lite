from exports import (PUBLIC as named, _PRIVATE)
from exports import *
from public_names import *
from pkg import child, MARKER as marker

try:
    from exports import PUBLIC as kept, MISSING
except ImportError as error:
    print(type(error).__name__)

try:
    _HIDDEN
except NameError:
    hidden = False
else:
    hidden = True

print(named, _PRIVATE, PUBLIC, EXPOSED, hidden, child.VALUE, marker, kept)
