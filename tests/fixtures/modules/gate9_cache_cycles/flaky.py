import state

state.ATTEMPTS += 1
if state.ATTEMPTS == 1:
    raise ValueError("first initialization failed")

VALUE = 9
