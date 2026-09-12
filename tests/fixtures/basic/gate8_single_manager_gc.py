class CycleManager:
    def __enter__(self):
        payload = [self]
        self.payload = payload
        return payload

    def __exit__(self, exc_type, exc, tb):
        return False

for outer in range(240):
    with CycleManager():
        junk = [outer, outer + 1, outer + 2, outer + 3]

print("context cycles collected")
