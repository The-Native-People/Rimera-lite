#!/usr/bin/env python3
import argparse
import json
import subprocess
import sys
import tomllib
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
FIXTURES = ROOT / "tests/fixtures/async/gate10_slice1"
BASELINE = ROOT / "spec/gates/10/baselines/macos-arm64-cpython-3.12.11.json"
BUDGETS = ROOT / "spec/gates/10/performance-budgets.toml"


def _load_json(path):
    return json.loads(path.read_text(encoding="utf-8"))


def _verify_snapshot(python, stem):
    result = subprocess.run(
        [str(python), str(FIXTURES / f"{stem}.py")],
        cwd=ROOT,
        check=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    if result.stderr:
        raise AssertionError(f"{stem} wrote unexpected stderr: {result.stderr}")
    actual = json.loads(result.stdout)
    expected = _load_json(FIXTURES / f"{stem}.expected.json")
    if actual != expected:
        raise AssertionError(
            f"{stem} differs from the checked-in CPython 3.12 snapshot"
        )


def _verify_baseline():
    baseline = _load_json(BASELINE)
    budgets = tomllib.loads(BUDGETS.read_text(encoding="utf-8"))
    policy = budgets["measurement_policy"]
    measured = budgets["measured"]["macos_arm64"]
    values = baseline["measurements"]

    if baseline.get("schema") != 1:
        raise AssertionError("unknown Gate 10 baseline schema")
    if not baseline["environment"]["python"].startswith("3.12.11 "):
        raise AssertionError("baseline was not recorded with CPython 3.12.11")
    if baseline["parameters"]["samples"] < policy["samples_min"]:
        raise AssertionError("baseline has too few samples")
    if baseline["parameters"]["warmups"] < policy["warmups_min"]:
        raise AssertionError("baseline has too few warmups")

    sync = baseline["sync_artifact"]
    if sync["status"] != "ok" or sync["async_symbols"]:
        raise AssertionError("synchronous baseline contains async/backend symbols")

    checks = [
        (
            values["coroutine_creation_ns_per_op"]["p95"],
            measured["coroutine_creation_p95_ns_max"],
            "coroutine creation p95",
        ),
        (
            values["direct_await_ready_ns_per_op"]["p95"],
            measured["direct_await_ready_p95_ns_max"],
            "direct await p95",
        ),
        (
            values["ready_task_handoff_ns_per_op"]["p95"],
            measured["ready_task_handoff_p95_ns_max"],
            "task handoff p95",
        ),
        (
            values["timer_overshoot_ns"]["p95"],
            measured["timer_1ms_overshoot_p95_ns_max"],
            "timer overshoot p95",
        ),
        (
            values["cancellation_delivery_ns"]["p95"],
            measured["cancellation_delivery_p95_ns_max"],
            "cancellation p95",
        ),
        (
            values["idle_task_memory"]["peak_bytes_per_task"],
            measured["idle_task_total_bytes_max"],
            "idle task memory",
        ),
    ]
    for actual, limit, name in checks:
        if actual > limit:
            raise AssertionError(f"reference {name} {actual} exceeds frozen limit {limit}")

    throughput = values["direct_await_throughput_ops_per_second"]["p50"]
    minimum = measured["direct_await_throughput_p50_ops_per_second_min"]
    if throughput < minimum:
        raise AssertionError(
            f"reference direct-await throughput {throughput} is below {minimum}"
        )


def parse_args():
    parser = argparse.ArgumentParser(description="Verify Gate 10 Slice 1 evidence.")
    parser.add_argument(
        "--python",
        type=Path,
        default=Path("/opt/homebrew/bin/python3.12"),
        help="CPython 3.12.11 executable used for oracle snapshots",
    )
    return parser.parse_args()


def main():
    args = parse_args()
    version = subprocess.run(
        [str(args.python), "--version"],
        check=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
    ).stdout.strip()
    if version != "Python 3.12.11":
        raise SystemExit(f"expected Python 3.12.11, got {version}")

    _verify_snapshot(args.python, "syntax_oracle")
    _verify_snapshot(args.python, "protocol_oracle")
    _verify_baseline()
    print("Gate 10 Slice 1 preparatory evidence verified")


if __name__ == "__main__":
    main()
