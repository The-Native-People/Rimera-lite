# Schema based validation

Rimera includes simple runtime-native schema validation.

```py
from rimera import Schema

class User(Schema):
    name: str
    age: int
    active: bool = True
```

Define a schema, simple as that.

```py
user = User.validate({
    "name": "Nadhi",
    "age": 17
})
```

Rimera validates the object against the schema and returns a validated `User`.

```py
print(user.name)
# Nadhi

print(user.age)
# 17

print(user.active)
# True
```

## Validation Errors

Rimera validation errors use native compiler diagnostics instead of dumping a large internal traceback.

```py
user = User.validate({
    "name": "Nadhi",
    "age": "seventeen"
})
```

```text
error[RIM-VAL-001]: validation failed

  × Invalid value for `User.age`
  │
  │ expected  int
  │ received  str
  │
  ╭─ tests/fixtures/basic/check_error.py:28:12
  │
28│     "age": "seventeen"
  │            ^^^^^^^^^^^ expected `int`, found `str`
  ╰─

  help: `User.age` is defined as `int`
```

The error should point directly to the value that failed validation.

Not:

```text
user = User.validate({
^
```

but:

```text
"age": "seventeen"
       ^^^^^^^^^^^
```

---

## Missing Fields

```py
user = User.validate({
    "name": "Nadhi"
})
```

```text
error[RIM-VAL-002]: missing required field

  × `User.age` is required
  │
  ╭─ tests/fixtures/basic/check_error.py:28:8
  │
28│ user = User.validate({
  │        ^^^^^^^^^^^^^
29│     "name": "Nadhi"
30│ })
  │ ──^ `age` was not provided
  ╰─

  help: add the required `age: int` field
```

---

## Multiple Validation Errors

Rimera should report multiple invalid fields together instead of forcing the program to be fixed one error at a time.

```py
user = User.validate({
    "name": 123,
    "age": "seventeen",
    "active": None
})
```

```text
error[RIM-VAL-003]: 3 fields failed validation

  × `User.name`
  │ expected  str
  │ received  int
  │
  ╭─ check_error.py:29:13
29│     "name": 123,
  │             ^^^ expected `str`
  ╰─

  × `User.age`
  │ expected  int
  │ received  str
  │
  ╭─ check_error.py:30:12
30│     "age": "seventeen",
  │            ^^^^^^^^^^^ expected `int`
  ╰─

  × `User.active`
  │ expected  bool
  │ received  None
  │
  ╭─ check_error.py:31:15
31│     "active": None
  │               ^^^^ expected `bool`
  ╰─
```

---

## Nested Schemas

Validation paths should show exactly where an error happened.

```py
class Address(Schema):
    city: str
    postcode: int


class User(Schema):
    name: str
    address: Address
```

```py
User.validate({
    "name": "Nadhi",
    "address": {
        "city": "Colombo",
        "postcode": "10000"
    }
})
```

```text
error[RIM-VAL-001]: validation failed

  × Invalid value for `User.address.postcode`
  │
  │ expected  int
  │ received  str
  │
  ╭─ user.py:34:21
34│         "postcode": "10000"
  │                     ^^^^^^^ expected `int`, found `str`
  ╰─
```

For arrays, validation paths should also include the failing index.

```text
User.friends[4].age
```

---

## Validation Model

Initial schema validation should support:

```text
str
int
float
bool
None

T | None

list[T]
dict[K, V]

nested Schema

required fields
default values
```

Example:

```py
class User(Schema):
    name: str
    age: int
    tags: list[str]
    metadata: dict[str, str]
    nickname: str | None = None
    active: bool = True
```

---

## Goal

Schema validation should remain small.

```py
class User(Schema):
    name: str
    age: int
```

```py
user = User.validate(data)
```

No configuration required.

No separate schema declaration.

No external validation package.

Rimera uses the type information already present in the class to build the validator.

```text
Schema definition
      ↓
Rimera type metadata
      ↓
Native validator
      ↓
Validated object
```

And when validation fails, Rimera should produce a normal Rimera source diagnostic pointing directly at the invalid value instead of exposing validator internals through a Python-style traceback.
