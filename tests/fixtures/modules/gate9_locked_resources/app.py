import importlib.resources
import pkg

print(importlib.resources.read_binary(pkg, "message.txt"))
