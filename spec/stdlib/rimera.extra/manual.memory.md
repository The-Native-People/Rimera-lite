# Experimental Manual Memory Management

Rimera normally uses automatic memory management just like Python.

For performance critical programs or sections, Rimera will also provide an **experimental Zig-like manual memory mode**.

This does not replace the Rimera GC.

It allows specific native memory allocations to be manually controlled by the programmer.

---

## Normal Garbage Collected Memory

Normal Rimera objects continue to work like Python.

```py
data = bytearray(4096)

data[0] = 100

# No manual free is required.
# Rimera automatically manages the lifetime.
```

```text
Rimera GC
└── Object
    └── Memory

Object becomes unreachable
        ↓
GC automatically releases it
```

Normal types such as:

```text
list
dict
str
bytes
class instances
functions
```

continue to be automatically managed.

---

## Manual Memory

Manual memory is allocated outside the normal Rimera GC.

```py
from rimera.extra import Allocator

alloc = Allocator.system()

buf = alloc.alloc(4096)

buf.write_u32(100)
buf.write_f64(3.14)

alloc.free(buf)

alloc.deinit()
```

```text
Rimera GC
└── Small memory handle
        │
        ▼
System Allocator
        │
        ▼
Native RAM
└── [ 4096 raw bytes ]
```

The programmer controls when this memory is released.

```py
buf = alloc.alloc(4096)

# Memory exists.

alloc.free(buf)

# Memory is now invalid.
```

Using `buf` after it has been freed should raise an error in safe/debug builds.

---

## Allocator

The base manual memory API is intentionally small.

```py
alloc.alloc(size)

alloc.alloc_zeroed(size)

alloc.resize(mem, new_size)

alloc.free(mem)

alloc.deinit()
```

### Allocate

```py
buf = alloc.alloc(4096)
```

Allocates native memory.

The contents are not guaranteed to be zeroed.

---

### Allocate Zeroed

```py
buf = alloc.alloc_zeroed(4096)
```

Allocates native memory and initializes it to zero.

---

### Resize

```py
buf = alloc.alloc(4096)

buf = alloc.resize(buf, 8192)
```

Resizes an existing allocation.

The returned allocation may have a different native address.

---

### Free

```py
alloc.free(buf)
```

Immediately releases the allocation.

---

### Deinit

```py
alloc.deinit()
```

Destroys the allocator.

In debug mode Rimera should report allocations that were never freed.

```text
Memory Leak

2 allocations were not freed
8192 bytes remain allocated
```

---

# Arena

Rimera also provides an `Arena` for performance critical sections that create many temporary allocations.

```py
from rimera.extra import Arena

arena = Arena()

a = arena.alloc(4096)
b = arena.alloc(2048)
c = arena.alloc(1024)

arena.deinit()
```

Instead of freeing every allocation individually, an Arena can release everything it owns together.

```text
Arena
┌─────────────────────────────────────┐
│ aaaa │ bb │ c │ free space         │
└─────────────────────────────────────┘

arena.deinit()
        ↓
All memory released
```

This makes allocations extremely cheap and is useful when many temporary objects share the same lifetime.

---

## Arena Scope

Arena memory can also be scoped.

```py
from rimera.extra import Arena

with Arena() as arena:
    temp = arena.alloc(4096)

    temp.write_u32(100)

# All allocations owned by the Arena
# are released here.
```

This is useful for:

```text
Parsers
Compilers
Serialization
Large calculations
Temporary buffers
Native data processing
Performance critical loops
```

---

# Buffer Integration

Manual allocations should use Rimera's native `Buffer` model.

```py
alloc = Allocator.system()

buf = alloc.alloc(4096)

buf.write_u32(100)

view = buf.slice(0, 128)

alloc.free(buf)
```

`alloc()` returns a manually-owned Buffer instead of exposing raw pointers directly.

```text
Buffer
├── ptr
├── len
├── capacity
├── allocator
└── ownership state
```

This keeps manual memory usable while still allowing Rimera to perform safety checks.

---

# Alignment

Manual allocations may request explicit alignment.

```py
buf = alloc.alloc(
    4096,
    alignment=64
)
```

This is useful for:

```text
SIMD
FFI
Native libraries
Cache aligned data
Low level I/O
```

---

# Safety

Manual memory is considered an experimental low-level feature.

Normal Rimera code remains memory managed.

Only objects explicitly allocated through a manual allocator use manual lifetime management.

```py
# GC managed
items = [1, 2, 3]

# Manually managed
buf = alloc.alloc(4096)
```

Rimera should prevent manual allocators from directly owning normal Python objects.

```py
# Not allowed
alloc.alloc_object(SomePythonClass)
```

Manual allocation is intended for native memory types such as:

```text
Buffer
PackedArray
NativeStruct
MemoryRegion
Raw native data
```

---

# Debug Safety

Debug builds should track manual allocations.

```py
alloc = Allocator.system()

buf = alloc.alloc(4096)

alloc.free(buf)

buf.write_u32(100)
```

Should produce:

```text
MemoryAccessError

Attempted to access memory after it was freed.
```

Double frees should also be detected.

```py
alloc.free(buf)
alloc.free(buf)
```

```text
MemoryAccessError

Attempted to free an allocation twice.
```

Allocator shutdown should detect leaks.

```py
alloc = Allocator.system()

buf = alloc.alloc(4096)

alloc.deinit()
```

```text
Memory Leak

1 allocation was not released
4096 bytes remain allocated
```

---

# Goal

Manual memory management is not intended to replace Rimera's GC.

It exists for sections where predictable allocation and memory lifetime can improve performance.

```text
Normal Rimera code
        ↓
Garbage Collected

Performance critical section
        ↓
Allocator / Arena
        ↓
Native memory
        ↓
Explicit lifetime
```

This gives Rimera both:

```text
Python-like automatic memory management

+

Zig-like explicit native memory control
```

without requiring the entire program to use manual memory management.
