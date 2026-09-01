# Rimera Extended
Rimera's Extended is a way to work with the hardcore hardware inside Rimera.

```py
from rimera import gc

heap = gc.new(
    max_heap=512 * gc.MB,
    profile=gc.PROFILE_THROUGHPUT,
)

heap.generations = 3

heap.gen0.size = 16 * gc.MB
heap.gen1.size = 64 * gc.MB

heap.pause_target = gc.ms(2)
heap.autotune = True

heap.profiler.enable()


# Permanent application structures

routes = build_routes()
gc.freeze(routes)


# Temporary allocations

def handle_request(req):

    with gc.arena():
        body = parse_json(req.body)
        result = process(body)

        # Escape something from the arena.
        return gc.escape(result)


# Latency-critical work

with gc.no_collect(max_alloc=8 * gc.MB):
    execute_critical_section()


print(heap.stats())
print(heap.profiler.report())
```