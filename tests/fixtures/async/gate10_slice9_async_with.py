events = []

class Pause:
    def __init__(self, label):
        self.label = label

    def __await__(self):
        resumed = yield self.label
        return resumed

class Manager:
    def __init__(self, name, suppress=False, fail_enter=False, fail_exit=False):
        self.name = name
        self.suppress = suppress
        self.fail_enter = fail_enter
        self.fail_exit = fail_exit
        # Special lookup must ignore instance shadowing.
        self.__aenter__ = "instance-shadow-enter"
        self.__aexit__ = "instance-shadow-exit"

    async def __aenter__(self):
        events.append(self.name + ":enter-start")
        await Pause(self.name + ":enter-wait")
        if self.fail_enter:
            raise ValueError(self.name + "-enter")
        events.append(self.name + ":entered")
        return self.name

    async def __aexit__(self, exc_type, exc, tb):
        type_name = "None"
        if exc_type is not None:
            type_name = exc_type.__name__
        events.append(self.name + ":exit:" + type_name + ":" + str(tb is not None))
        await Pause(self.name + ":exit-wait")
        events.append(self.name + ":exited")
        if self.fail_exit:
            raise RuntimeError(self.name + "-exit")
        return self.suppress

class SyncManager:
    def __init__(self, name):
        self.name = name

    def __enter__(self):
        events.append(self.name + ":sync-enter")
        return self.name

    def __exit__(self, exc_type, exc, tb):
        type_name = "None"
        if exc_type is not None:
            type_name = exc_type.__name__
        events.append(self.name + ":sync-exit:" + type_name)
        return False

class CancelSignal(BaseException):
    pass

def drain(label, coroutine):
    while True:
        try:
            token = coroutine.send(None)
            print(label, "suspend", token)
        except StopIteration as exc:
            print(label, "return", exc.value)
            return
        except BaseException as exc:
            print(label, "raise", type(exc).__name__, str(exc))
            return

async def normal_multiple():
    async with Manager("A") as a, Manager("B") as b:
        events.append("body:" + a + b)
    return "normal-done"

drain("normal", normal_multiple())
print("normal-events", events)
events.clear()

async def partial_entry():
    try:
        async with Manager("A"), Manager("B", fail_enter=True):
            events.append("unreachable")
    except ValueError as exc:
        events.append("caught:" + str(exc))
    return "partial-done"

drain("partial", partial_entry())
print("partial-events", events)
events.clear()

async def suppressed_body():
    async with Manager("S", suppress=True):
        raise ValueError("body-error")
    events.append("suppressed")
    return 11

drain("suppress", suppressed_body())
print("suppress-events", events)
events.clear()

async def replacement_exit():
    try:
        async with Manager("R", fail_exit=True):
            raise ValueError("body-error")
    except RuntimeError as exc:
        context_name = "None"
        if exc.__context__ is not None:
            context_name = type(exc.__context__).__name__
        events.append("replacement:" + context_name)
        names = []
        traceback = exc.__traceback__
        while traceback is not None:
            names.append(traceback.tb_frame.f_code.co_name)
            traceback = traceback.tb_next
        events.append("replacement-trace:" + str(names))
    return 12

drain("replace", replacement_exit())
print("replace-events", events)
events.clear()

async def return_transfer():
    async with Manager("RET"):
        return 77

drain("return", return_transfer())
print("return-events", events)
events.clear()

async def loop_transfer():
    index = 0
    while index < 3:
        async with Manager("L" + str(index)):
            index += 1
            if index == 1:
                continue
            if index == 2:
                break
    return index

drain("loop", loop_transfer())
print("loop-events", events)
events.clear()

async def target_failure():
    try:
        async with Manager("T") as (left, right):
            events.append("target-body")
    except BaseException as exc:
        events.append("target-error:" + type(exc).__name__)
    return "target-done"

drain("target", target_failure())
print("target-events", events)
events.clear()

async def nested_sync_async():
    async with Manager("N"):
        with SyncManager("SYNC"):
            await Pause("nested-body-wait")
    return "nested-done"

drain("nested", nested_sync_async())
print("nested-events", events)
events.clear()

async def cancelled_body():
    try:
        async with Manager("C"):
            await Pause("cancel-body-wait")
    except CancelSignal as exc:
        events.append("cancel-caught:" + str(exc))
    return "cancel-done"

cancelled = cancelled_body()
print("cancel suspend", cancelled.send(None))
print("cancel suspend", cancelled.send(None))
try:
    print("cancel suspend", cancelled.throw(CancelSignal("stop")))
except StopIteration as exc:
    print("cancel return", exc.value)
else:
    try:
        cancelled.send(None)
    except StopIteration as exc:
        print("cancel return", exc.value)
print("cancel-events", events)
events.clear()

class CycleManager:
    def __init__(self):
        self.cycle = self

    async def __aenter__(self):
        return 1

    async def __aexit__(self, exc_type, exc, tb):
        return False

async def cycle_stress():
    index = 0
    total = 0
    while index < 1500:
        async with CycleManager() as value:
            total += value
        index += 1
    return total

drain("cycles", cycle_stress())

# Binding descriptors is observable and exit must stay captured even if its
# class entry changes while entry is suspended.
class MethodDescriptor:
    def __init__(self, name):
        self.name = name

    def __get__(self, instance, owner):
        events.append("lookup:" + self.name)
        if self.name == "enter":
            return instance.enter
        return instance.exit

class DescriptorManager:
    __aenter__ = MethodDescriptor("enter")
    __aexit__ = MethodDescriptor("exit")

    async def enter(self):
        DescriptorManager.__aexit__ = None
        await Pause("descriptor-enter")
        return self

    async def exit(self, kind, value, traceback):
        events.append("captured-exit:" + str(kind is None))
        return TruthResult(False)

class TruthResult:
    def __init__(self, fail):
        self.fail = fail

    def __bool__(self):
        events.append("truth")
        if self.fail:
            raise RuntimeError("truth-failed")
        return True

async def descriptor_order():
    async with DescriptorManager():
        events.append("descriptor-body")

drain("descriptor", descriptor_order())
print("descriptor-events", events)
events.clear()

async def custom_suppression(fail):
    try:
        async with Manager("TRUTH", suppress=TruthResult(fail)):
            raise ValueError("truth-body")
    except RuntimeError as exc:
        events.append("truth-context:" + type(exc.__context__).__name__)

drain("truth", custom_suppression(False))
drain("truth-fail", custom_suppression(True))
print("truth-events", events)
events.clear()

class MissingBoth:
    pass

class MissingExit:
    async def __aenter__(self):
        events.append("must-not-enter")

async def missing_protocol(manager):
    try:
        async with manager:
            events.append("must-not-run")
    except TypeError as exc:
        print("missing-protocol", str(exc))

drain("missing-both", missing_protocol(MissingBoth()))
drain("missing-exit", missing_protocol(MissingExit()))

# Cancellation at partial entry unwinds only the previously entered manager;
# cancellation of an exit must never invoke that exit a second time.
async def cancellation_phase():
    try:
        async with Manager("OUTER"), Manager("INNER"):
            events.append("phase-body")
    except CancelSignal:
        events.append("phase-cancelled")

for phase in ("entry", "exit"):
    coroutine = cancellation_phase()
    print(phase, coroutine.send(None))
    print(phase, coroutine.send(None))
    if phase == "exit":
        print(phase, coroutine.send(None))
    print(phase, coroutine.throw(CancelSignal("phase-stop")))
    drain(phase, coroutine)
    print(phase + "-events", events)
    events.clear()
