import control

ready = True
if control.fail:
    raise RuntimeError("reload boom")
