#!/usr/bin/env python3
import argparse
import asyncio
import gc
import json
import platform
import re
import statistics
import subprocess
import sys
import time
import tracemalloc
from pathlib import Path


ASYNC_SYMBOL = re.compile(r"(?:^|_)(?:rimera_async|compio|monoio|tokio)(?:_|$)", re.I)


async def _immediate():
    return 1


async def _parent():
    return await _immediate()


def _finish_ready(coroutine):
    try:
        coroutine.send(None)
    except StopIteration as stop:
        return stop.value
    raise RuntimeError("ready-only benchmark unexpectedly suspended")


def _distribution(values):
    ordered = sorted(values)
    if not ordered:
        return {}

    def percentile(p):
        if len(ordered) == 1:
            return ordered[0]
        index = (len(ordered) - 1) * p
        lower = int(index)
        upper = min(lower + 1, len(ordered) - 1)
        weight = index - lower
        return ordered[lower] * (1.0 - weight) + ordered[upper] * weight

    return {
        "count": len(ordered),
        "min": ordered[0],
        "mean": statistics.fmean(ordered),
        "p50": percentile(0.50),
        "p95": percentile(0.95),
        "p99": percentile(0.99),
        "max": ordered[-1],
        "stdev": statistics.pstdev(ordered),
    }


def _sample_sync(operation, iterations, samples):
    values = []
    for _ in range(samples):
        gc.collect()
        start = time.perf_counter_ns()
        operation(iterations)
        elapsed = time.perf_counter_ns() - start
        values.append(elapsed / iterations)
    return values


def _coroutine_creation(iterations):
    for _ in range(iterations):
        coroutine = _immediate()
        coroutine.close()


def _direct_await(iterations):
    for _ in range(iterations):
        if _finish_ready(_parent()) != 1:
            raise RuntimeError("direct await benchmark returned the wrong value")


async def _ready_handoff_batch(iterations):
    start = time.perf_counter_ns()
    for _ in range(iterations):
        event = asyncio.Event()

        async def waiter():
            await event.wait()

        task = asyncio.create_task(waiter())
        await asyncio.sleep(0)
        event.set()
        await task
    return (time.perf_counter_ns() - start) / iterations


async def _timer_overshoot(iterations, delay_seconds):
    values = []
    delay_ns = int(delay_seconds * 1_000_000_000)
    for _ in range(iterations):
        start = time.perf_counter_ns()
        await asyncio.sleep(delay_seconds)
        elapsed = time.perf_counter_ns() - start
        values.append(max(0, elapsed - delay_ns))
    return values


async def _cancellation_latency(iterations):
    values = []
    for _ in range(iterations):
        event = asyncio.Event()

        async def waiter():
            await event.wait()

        task = asyncio.create_task(waiter())
        await asyncio.sleep(0)
        start = time.perf_counter_ns()
        task.cancel()
        try:
            await task
        except asyncio.CancelledError:
            pass
        values.append(time.perf_counter_ns() - start)
    return values


async def _idle_task_memory(task_count):
    gc.collect()
    tracemalloc.start()
    before, _ = tracemalloc.get_traced_memory()
    event = asyncio.Event()
    tasks = [asyncio.create_task(event.wait()) for _ in range(task_count)]
    await asyncio.sleep(0)
    current, peak = tracemalloc.get_traced_memory()
    for task in tasks:
        task.cancel()
    await asyncio.gather(*tasks, return_exceptions=True)
    tracemalloc.stop()
    return {
        "task_count": task_count,
        "current_bytes_per_task": max(0, current - before) / task_count,
        "peak_bytes_per_task": max(0, peak - before) / task_count,
    }


def _command_output(*args):
    try:
        result = subprocess.run(
            args,
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
        )
    except (OSError, subprocess.CalledProcessError):
        return None
    return result.stdout.strip()


def _artifact_record(path):
    if path is None:
        return {"status": "not-supplied"}
    artifact = Path(path)
    if not artifact.is_file():
        return {"status": "missing", "path": str(artifact)}
    symbols = _command_output("nm", str(artifact)) or ""
    async_symbols = []
    for line in symbols.splitlines():
        name = line.split()[-1] if line.split() else ""
        normalized = name[1:] if name.startswith("_") else name
        if ASYNC_SYMBOL.search(normalized):
            async_symbols.append(normalized)
    return {
        "status": "ok",
        "path": str(artifact),
        "size_bytes": artifact.stat().st_size,
        "async_symbols": sorted(set(async_symbols)),
    }


def _environment():
    return {
        "python": sys.version.replace("\n", " "),
        "python_executable": sys.executable,
        "platform": platform.platform(),
        "machine": platform.machine(),
        "processor": platform.processor(),
        "cpu": _command_output("sysctl", "-n", "machdep.cpu.brand_string"),
        "os_version": _command_output("sw_vers", "-productVersion"),
        "rustc": _command_output("rustc", "--version"),
        "cargo": _command_output("cargo", "--version"),
        "clang": (_command_output("clang", "--version") or "").splitlines()[0] or None,
    }


def run(args):
    for _ in range(args.warmups):
        _coroutine_creation(args.sync_iterations // 4)
        _direct_await(args.sync_iterations // 4)

    creation_samples = _sample_sync(
        _coroutine_creation, args.sync_iterations, args.samples
    )
    direct_samples = _sample_sync(_direct_await, args.sync_iterations, args.samples)
    creation = _distribution(creation_samples)
    direct = _distribution(direct_samples)
    direct_throughput = _distribution(
        [1_000_000_000 / value for value in direct_samples if value]
    )

    handoff = []
    for _ in range(args.samples):
        handoff.append(asyncio.run(_ready_handoff_batch(args.task_iterations)))

    timer = []
    for _ in range(args.samples):
        timer.extend(asyncio.run(_timer_overshoot(args.timer_iterations, args.timer_delay)))

    cancellation = []
    for _ in range(args.samples):
        cancellation.extend(asyncio.run(_cancellation_latency(args.task_iterations)))

    idle_memory = asyncio.run(_idle_task_memory(args.idle_tasks))

    return {
        "schema": 1,
        "oracle": "CPython 3.12",
        "environment": _environment(),
        "parameters": {
            "samples": args.samples,
            "warmups": args.warmups,
            "sync_iterations": args.sync_iterations,
            "task_iterations": args.task_iterations,
            "timer_iterations": args.timer_iterations,
            "timer_delay_seconds": args.timer_delay,
            "idle_tasks": args.idle_tasks,
        },
        "measurements": {
            "coroutine_creation_ns_per_op": creation,
            "direct_await_ready_ns_per_op": direct,
            "ready_task_handoff_ns_per_op": _distribution(handoff),
            "timer_overshoot_ns": _distribution(timer),
            "cancellation_delivery_ns": _distribution(cancellation),
            "idle_task_memory": idle_memory,
            "direct_await_throughput_ops_per_second": direct_throughput,
        },
        "sync_artifact": _artifact_record(args.sync_artifact),
        "notes": [
            "CPython asyncio task/timer measurements are an external baseline only; Gate 10 does not claim asyncio compatibility.",
            "Rimera structural budgets are checked separately from these platform-sensitive timing distributions.",
            "No Rimera async implementation is exercised by this Slice 1 baseline harness.",
        ],
    }


def parse_args():
    parser = argparse.ArgumentParser(
        description="Record the Gate 10 pre-implementation async baseline."
    )
    parser.add_argument("--output", type=Path)
    parser.add_argument("--sync-artifact", type=Path)
    parser.add_argument("--samples", type=int, default=7)
    parser.add_argument("--warmups", type=int, default=2)
    parser.add_argument("--sync-iterations", type=int, default=20_000)
    parser.add_argument("--task-iterations", type=int, default=300)
    parser.add_argument("--timer-iterations", type=int, default=16)
    parser.add_argument("--timer-delay", type=float, default=0.001)
    parser.add_argument("--idle-tasks", type=int, default=1_000)
    return parser.parse_args()


def main():
    args = parse_args()
    if sys.version_info[:2] != (3, 12):
        raise SystemExit("Gate 10 baseline must run under CPython 3.12")
    report = run(args)
    encoded = json.dumps(report, indent=2, sort_keys=True) + "\n"
    if args.output is not None:
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(encoded, encoding="utf-8")
    else:
        sys.stdout.write(encoded)


if __name__ == "__main__":
    main()
