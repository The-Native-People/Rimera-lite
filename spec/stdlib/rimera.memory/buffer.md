## Direct Buffer allows you to allocate buffers directly the Gc

Rimera GC heap
└── Buffer object
    ├── ptr
    ├── len
    ├── capacity
    └── ownership metadata
          │
          ▼
    Native memory
    └── [ 4096 raw bytes ]

```py
# Directly create a buffer in Memory.
# This is allocated to system ram.
buf = Buffer.alloc(4096)

buf.write_u32(100)
buf.write_f64(3.14)

view = buf.slice(0, 128)
```

