import mesh
import mesh.left
import mesh.right

print(mesh.__name__)
print(mesh.__package__)
print(mesh.__file__)
print(mesh.__loader__ is mesh.__spec__.loader)
print(mesh.__spec__.origin)
print(mesh.__spec__.has_location)
print(mesh.__spec__.submodule_search_locations is mesh.__path__)
print(len(mesh.__path__))
print(mesh.left.value)
print(mesh.right.value)
print(mesh.left is mesh.left)
