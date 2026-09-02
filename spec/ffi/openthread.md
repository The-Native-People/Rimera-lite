# Openthread
## Go-like channels but anyone can listen in.

```py
from rimrea import openthread as ot

# Fx can be performance, default or efficient
# Performance will dedicate resources for performance.
# and others will do for etc.
thread = ot.new(fx=performance)

# Start handshake + thread
ot.start("rimera-jsbridge")
ch.send("hello")
msg = ch.recv()
```

```py
from rimrea import openthread as ot

connector = ot.connect("rimrea-jsbridge")

for char in connector:
    print(f"Got new message {char}")
```