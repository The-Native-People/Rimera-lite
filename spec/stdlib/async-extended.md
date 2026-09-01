Extended standard library for Rimera uses threading model similar to Go-lang

##  Event loop driven
```py
from rimera import async_runtime

def main():
    print("Hello i might be something async")

async_runtime.run(main())
```

## Tasks

```py
from rimera import task

def worker():
    """
    Worker logic
    Logic to run workers
    """

task.spawn(worker)
```
