import cycle_a
import cycle_b
import cycle_a as cycle_a_again

print(cycle_a.FROM_B, cycle_b.SEEN_A)
print(cycle_a is cycle_a_again)

try:
    import flaky
except ValueError as error:
    print(type(error).__name__, str(error))

import flaky
import state

print(flaky.VALUE, state.ATTEMPTS)

try:
    import partial
except AttributeError as error:
    print(str(error))

try:
    import cycle_a as kept, broken as absent
except RuntimeError as error:
    print(kept is cycle_a, str(error))
