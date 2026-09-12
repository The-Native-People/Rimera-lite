import event_log
import pkg
import pkg.sub.leaf as leaf

print(event_log.events)
print(pkg.__name__, pkg.__package__, pkg.__file__, pkg.__path__[0])
print(type(pkg.__loader__).__name__, pkg.__loader__.name, pkg.__loader__.path)
print(
    type(pkg.__spec__).__name__,
    pkg.__spec__.name,
    pkg.__spec__.parent,
    pkg.__spec__.loader is pkg.__loader__,
    pkg.__spec__.submodule_search_locations is pkg.__path__,
    pkg.__spec__.has_location,
)
print(leaf.__name__, leaf.__package__, leaf.VALUE)
print(leaf.__spec__.parent, leaf.__spec__.loader is leaf.__loader__)
print(leaf.sibling is pkg.sibling, leaf.peer.leaf is leaf)
