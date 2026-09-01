class X:
    pass


class Y:
    pass


class A(X, Y):
    pass


class B(Y, X):
    pass


class Invalid(A, B):
    pass
