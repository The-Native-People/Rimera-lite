def make_recursive(prefix):
    def recurse(depth, suffix="survived"):
        if depth <= 0:
            return prefix + suffix
        return recurse(depth - 1, suffix)

    return recurse


recursive = make_recursive("managed values ")
print(recursive(600))
