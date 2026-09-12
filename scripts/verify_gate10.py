#!/usr/bin/env python3
"""Measure native Gate 10 without weakening the frozen release thresholds."""
import argparse
import hashlib
import json
import os
import shutil
import subprocess
import tempfile
import time
import tomllib
from pathlib import Path

from gate10_async_baseline import _distribution, _environment

ROOT = Path(__file__).resolve().parents[1]
FIXTURES = ROOT / "tests/fixtures/async"
PYTHON = Path("/opt/homebrew/bin/python3.12")


def command(arguments, **kwargs):
    result = subprocess.run([str(arg) for arg in arguments], cwd=ROOT,
                            capture_output=True, timeout=120, **kwargs)
    if result.returncode:
        raise RuntimeError(f"{arguments}: {result.stderr.decode(errors='replace')}")
    return result


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def build(source, output, profile, backend="auto", heap_limit_bytes=None):
    cache_root = output.parent / (output.name + "-cache")
    cache = cache_root / ".rimera"
    # Reproducibility evidence must come from independent compiler work, not a
    # second publication of cached objects/manifests from the first build.
    if output.exists():
        output.unlink()
    shutil.rmtree(cache_root, ignore_errors=True)
    arguments = [ROOT / "target/release/rimera-lite", source, "--profile", profile,
                 "--async", backend, "--output", output]
    if heap_limit_bytes is not None:
        arguments.extend(["--heap-limit-bytes", str(heap_limit_bytes)])
    command(arguments, env={**os.environ, "RIMERA_CACHE_DIR": str(cache)})
    return cache


def cache_hashes(cache):
    paths = [*cache.rglob("*.o"), cache / "build-manifest.toml"]
    if len(paths) < 2:
        raise AssertionError("native object missing from reproducibility audit")
    return {str(path.relative_to(cache)): digest(path) for path in paths}


def symbols(path):
    names = command(["nm", path]).stdout.decode().lower()
    forbidden = ["pyobject", "_py_", "rustpython", "setjmp", "longjmp", "rimera_compat"]
    found = [name for name in forbidden if name in names]
    if found:
        raise AssertionError(f"{path}: forbidden symbols {found}")
    return names


def sample_program(arguments, expected, iterations):
    """Time one whole-process source workload with two warmups and seven samples."""
    samples = []
    for index in range(9):
        start = time.perf_counter_ns()
        result = command(arguments)
        elapsed = time.perf_counter_ns() - start
        if result.stdout != expected or result.stderr:
            raise AssertionError(f"unexpected benchmark result: {result}")
        if index >= 2:
            samples.append(elapsed / iterations)
    return {"samples": samples, **_distribution(samples)}


def run():
    budgets = tomllib.loads((ROOT / "spec/gates/10/performance-budgets.toml").read_text())
    limits = budgets["measured"]["macos_arm64"]
    environment = _environment()
    python_version = command([PYTHON, "--version"]).stdout.decode().strip()
    rust_host = command(["rustc", "-vV"]).stdout.decode()
    if python_version != "Python 3.12.11":
        raise AssertionError(f"Gate 10 audit requires CPython 3.12.11, got {python_version}")
    if environment.get("machine") != "arm64":
        raise AssertionError(f"Gate 10 audit requires macOS arm64, got {environment.get('machine')}")
    if "host: aarch64-apple-darwin" not in rust_host:
        raise AssertionError("Gate 10 audit requires the aarch64-apple-darwin Rust host")
    report = {"schema": 1, "environment": environment, "backend": "compio 0.17.0",
              "parameters": {"warmups": 2, "samples": 7, "abi_iterations": 20000,
                             "native_await_iterations": 250000,
                             "slice13_pure_ready_iterations": 1000000,
                             "slice13_creation_close_iterations": 1000000},
              "profiles": {}, "failures": []}
    source = FIXTURES / "gate10_slice11/bench_await_ready.py"
    expected = command([PYTHON, source]).stdout
    cpython_same_source = sample_program([PYTHON, source], expected, 250000)
    slice13_source = FIXTURES / "gate10_slice13/bench_pure_ready_repeat.py"
    slice13_expected = command([PYTHON, slice13_source]).stdout
    slice13_cpython = sample_program([PYTHON, slice13_source], slice13_expected, 1000000)
    creation_source = FIXTURES / "gate10_slice13/bench_creation_close.py"
    creation_expected = command([PYTHON, creation_source]).stdout
    creation_cpython = sample_program([PYTHON, creation_source], creation_expected, 1000000)
    cpython_reference = json.loads(
        (ROOT / "spec/gates/10/baselines/macos-arm64-cpython-3.12.11.json").read_text()
    )["measurements"]
    with tempfile.TemporaryDirectory(prefix="rimera-gate10-audit-") as temporary:
        folder = Path(temporary)
        for profile in ("debug", "release"):
            print(f"Measuring {profile}", flush=True)
            raw = command([ROOT / f"target/{profile}/examples/gate10_bench"]).stdout.decode()
            values = {}
            for line in raw.splitlines():
                key, value = line.split()
                values.setdefault(key, []).append(float(value))
            measurements = {key: {"samples": samples, **_distribution(samples)}
                            for key, samples in values.items()}
            artifact = folder / f"await-{profile}"
            cache = build(source, artifact, profile)
            measurements["direct_await_ready_ns_per_op"] = sample_program([artifact], expected, 250000)
            throughput = [1e9 / sample for sample in measurements["direct_await_ready_ns_per_op"]["samples"]]
            measurements["direct_await_throughput_ops_per_second"] = {
                "samples": throughput, **_distribution(throughput)}
            first = digest(artifact)
            first_cache = cache_hashes(cache)
            build(source, artifact, profile)
            second = digest(artifact)
            second_cache = cache_hashes(cache)
            if first != second:
                report["failures"].append(f"{profile}: executable is not byte reproducible")
            if first_cache != second_cache:
                report["failures"].append(f"{profile}: native objects/manifest are not reproducible")
            names = symbols(artifact)
            # Release artifacts are intentionally stripped, so the private
            # selector symbol is a valid assertion only on the debug artifact.
            if profile == "debug" and "rimera_async_backend_select_compio" not in names:
                raise AssertionError("debug async control lacks the static Compio entry")
            if "monoio" in names or "tokio" in names:
                raise AssertionError("more than one backend linked")
            report["profiles"][profile] = {"measurements": measurements,
                "reproducibility": {"first_sha256": first, "second_sha256": second,
                                    "first_cache": first_cache, "second_cache": second_cache}}
        slice13_artifact = folder / "slice13-pure-ready"
        build(slice13_source, slice13_artifact, "release")
        slice13_rimera = sample_program([slice13_artifact], slice13_expected, 1000000)
        creation_artifact = folder / "slice13-creation-close"
        build(creation_source, creation_artifact, "release")
        creation_rimera = sample_program([creation_artifact], creation_expected, 1000000)
        fallback_results = {}
        for fallback_name in ("impure_ready_fallback.py", "rebound_ready_fallback.py"):
            fallback_source = FIXTURES / "gate10_slice13" / fallback_name
            fallback_expected = command([PYTHON, fallback_source]).stdout
            fallback_artifact = folder / fallback_name.removesuffix(".py")
            build(fallback_source, fallback_artifact, "release")
            fallback_result = command([fallback_artifact])
            if fallback_result.stdout != fallback_expected or fallback_result.stderr:
                raise AssertionError(f"Slice 13 fallback mismatch: {fallback_name}")
            fallback_results[fallback_name] = "matches CPython 3.12.11"
        for semantic_name in ("creation_close_semantics.py", "creation_close_rebound.py"):
            semantic_source = FIXTURES / "gate10_slice13" / semantic_name
            semantic_expected = command([PYTHON, semantic_source]).stdout
            semantic_artifact = folder / semantic_name.removesuffix(".py")
            build(semantic_source, semantic_artifact, "release")
            semantic_result = command([semantic_artifact])
            if semantic_result.stdout != semantic_expected or semantic_result.stderr:
                raise AssertionError(f"Slice 13 creation-close mismatch: {semantic_name}")
            fallback_results[semantic_name] = "matches CPython 3.12.11"
        lowheap_source = FIXTURES / "gate10_slice13/creation_close_semantics.py"
        lowheap_expected = command([PYTHON, lowheap_source]).stdout
        lowheap_artifact = folder / "creation-close-lowheap"
        build(lowheap_source, lowheap_artifact, "release", heap_limit_bytes=131072)
        lowheap_result = command([lowheap_artifact])
        if lowheap_result.stdout != lowheap_expected or lowheap_result.stderr:
            raise AssertionError("Slice 13 low-heap creation-close fallback mismatch")
        fallback_results["creation_close_lowheap"] = "matches CPython 3.12.11 with elision disabled"
        report["slice13_fallbacks"] = fallback_results
        sync = folder / "sync"
        asynchronous = folder / "async"
        sync_cache = build(FIXTURES / "gate10_slice10_sync_control.py", sync, "release")
        sync_hash = digest(sync)
        sync_cache_hashes = cache_hashes(sync_cache)
        sync_cache = build(FIXTURES / "gate10_slice10_sync_control.py", sync, "release")
        sync_repeat_hash = digest(sync)
        sync_repeat_cache_hashes = cache_hashes(sync_cache)
        if sync_repeat_hash != sync_hash:
            report["failures"].append("release synchronous executable is not byte reproducible")
        if sync_repeat_cache_hashes != sync_cache_hashes:
            report["failures"].append("release synchronous native objects/manifest are not reproducible")
        build(FIXTURES / "gate10_slice10_sync_control.py", sync, "release", "compio")
        if digest(sync) != sync_hash:
            report["failures"].append("explicit Compio changes a synchronous artifact")
        build(FIXTURES / "gate10_slice10_compio_root.py", asynchronous, "release")
        sync_names = symbols(sync)
        if any(name in sync_names for name in ("rimera_async", "compio", "tokio", "monoio")):
            raise AssertionError("synchronous artifact links async infrastructure")
        symbols(asynchronous)
        size_delta = asynchronous.stat().st_size - sync.stat().st_size
        report["artifacts"] = {"sync_bytes": sync.stat().st_size,
            "async_bytes": asynchronous.stat().st_size, "delta_bytes": size_delta,
            "sync_async_symbols": [], "native_only": True,
            "sync_reproducible": sync_repeat_hash == sync_hash and sync_repeat_cache_hashes == sync_cache_hashes,
            "sync_auto_compio_identical": digest(sync) == sync_hash}
    measured = report["profiles"]["release"]["measurements"]
    cpython_same_source_throughput = [
        1e9 / sample for sample in cpython_same_source["samples"]
    ]
    cpython_same_source_throughput_distribution = {
        "samples": cpython_same_source_throughput,
        **_distribution(cpython_same_source_throughput),
    }
    rimera_same_source = measured["direct_await_ready_ns_per_op"]
    report["same_source_comparison"] = {
        "source": str(source.relative_to(ROOT)),
        "iterations": 250000,
        "timing_scope": "whole process startup + source loop + output; no subtraction",
        "cpython_3_12_11_ns_per_op": cpython_same_source,
        "cpython_3_12_11_ops_per_second": cpython_same_source_throughput_distribution,
        "rimera_release_ns_per_op": rimera_same_source,
        "rimera_release_ops_per_second": measured["direct_await_throughput_ops_per_second"],
        "rimera_over_cpython_p50_time_ratio": (
            rimera_same_source["p50"] / cpython_same_source["p50"]
        ),
    }
    slice13_speedup = slice13_cpython["p50"] / slice13_rimera["p50"]
    creation_speedup = creation_cpython["p50"] / creation_rimera["p50"]
    report["slice13_creation_close_comparison"] = {
        "source": str(creation_source.relative_to(ROOT)),
        "source_create_close_iterations": 1000000,
        "timing_scope": "whole process startup + exact Python source workload + output; no subtraction",
        "cpython_3_12_11_effective_ns_per_source_create_close": creation_cpython,
        "rimera_release_effective_ns_per_source_create_close": creation_rimera,
        "rimera_over_cpython_p50_speedup": creation_speedup,
        "required_rimera_p50_ns_max": limits["slice13_creation_close_same_source_p50_ns_max"],
        "materialized_runtime_diagnostic": measured["coroutine_creation_ns_per_op"],
    }
    report["slice13_same_source_comparison"] = {
        "source": str(slice13_source.relative_to(ROOT)),
        "source_await_iterations": 1000000,
        "timing_scope": "whole process startup + source workload + output; no subtraction",
        "optimization_scope": (
            "source-level await count; Rimera may AOT-collapse only dynamically verified "
            "repeat-pure, non-suspending zero-argument coroutines over an exact managed range"
        ),
        "cpython_3_12_11_effective_ns_per_source_await": slice13_cpython,
        "rimera_release_effective_ns_per_source_await": slice13_rimera,
        "rimera_over_cpython_p50_speedup": slice13_speedup,
        "required_p50_speedup": limits["slice13_pure_ready_same_source_speedup_p50_min"],
    }
    comparison_matrix = {
        "ready_await_same_source": {
            "direction": "lower",
            "cpython_p50": cpython_same_source["p50"],
            "rimera_p50": rimera_same_source["p50"],
        },
        "creation_close_same_source": {
            "direction": "lower",
            "cpython_p50": creation_cpython["p50"],
            "rimera_p50": creation_rimera["p50"],
        },
        "pure_ready_repeat_same_source": {
            "direction": "lower",
            "cpython_p50": slice13_cpython["p50"],
            "rimera_p50": slice13_rimera["p50"],
        },
        "ready_task_handoff": {
            "direction": "lower",
            "cpython_p50": cpython_reference["ready_task_handoff_ns_per_op"]["p50"],
            "rimera_p50": measured["ready_task_handoff_ns_per_op"]["p50"],
        },
        "timer_overshoot": {
            "direction": "lower",
            "cpython_p50": cpython_reference["timer_overshoot_ns"]["p50"],
            "rimera_p50": measured["timer_overshoot_ns"]["p50"],
        },
        "cancellation_delivery": {
            "direction": "lower",
            "cpython_p50": cpython_reference["cancellation_delivery_ns"]["p50"],
            "rimera_p50": measured["cancellation_delivery_ns"]["p50"],
        },
        "ready_await_throughput": {
            "direction": "higher",
            "cpython_p50": cpython_same_source_throughput_distribution["p50"],
            "rimera_p50": measured["direct_await_throughput_ops_per_second"]["p50"],
        },
    }
    report["cpython_comparison_matrix"] = comparison_matrix
    for name, comparison in comparison_matrix.items():
        loses = (
            comparison["rimera_p50"] >= comparison["cpython_p50"]
            if comparison["direction"] == "lower"
            else comparison["rimera_p50"] <= comparison["cpython_p50"]
        )
        if loses:
            report["failures"].append(
                f"{name}: Rimera p50 {comparison['rimera_p50']} does not beat "
                f"CPython p50 {comparison['cpython_p50']}"
            )
    for key, limit in [
        ("coroutine_creation_ns_per_op", "coroutine_creation_p95_ns_max"),
        ("direct_await_ready_ns_per_op", "direct_await_ready_p95_ns_max"),
        ("ready_task_handoff_ns_per_op", "ready_task_handoff_p95_ns_max"),
        ("timer_overshoot_ns", "timer_1ms_overshoot_p95_ns_max"),
        ("cancellation_delivery_ns", "cancellation_delivery_p95_ns_max"),
        ("idle_task_total_bytes", "idle_task_total_bytes_max"),
    ]:
        if measured[key]["p95"] > limits[limit]:
            report["failures"].append(f"{key}: {measured[key]['p95']} > {limits[limit]}")
    if measured["direct_await_throughput_ops_per_second"]["p50"] < limits["direct_await_throughput_p50_ops_per_second_min"]:
        report["failures"].append("direct-await throughput below frozen minimum")
    if report["artifacts"]["delta_bytes"] > limits["async_release_artifact_size_delta_bytes_max"]:
        report["failures"].append("async release size delta exceeds frozen maximum")
    if creation_rimera["p50"] >= limits["slice13_creation_close_same_source_p50_ns_max"]:
        report["failures"].append(
            f"Slice 13 creation-close p50 {creation_rimera['p50']:.3f} ns >= "
            f"{limits['slice13_creation_close_same_source_p50_ns_max']:.3f} ns"
        )
    if creation_rimera["p50"] >= creation_cpython["p50"]:
        report["failures"].append(
            f"Slice 13 creation-close does not beat CPython: "
            f"{creation_rimera['p50']:.3f} ns >= {creation_cpython['p50']:.3f} ns"
        )
    if slice13_speedup < limits["slice13_pure_ready_same_source_speedup_p50_min"]:
        report["failures"].append(
            f"Slice 13 pure-ready speedup {slice13_speedup:.3f}x < "
            f"{limits['slice13_pure_ready_same_source_speedup_p50_min']:.3f}x"
        )
    report["notes"] = [
        "Slice 13's creation-close headline is the exact same Python source under both runtimes. Rimera may AOT-elide nonescaping unstarted coroutine lifecycles only behind exact range/callable guards; the final iteration remains materialized and low-heap/rebound cases must fall back.",
        "The materialized coroutine ABI creation+close metric remains reported separately as a diagnostic and is not relabeled as the <10 ns source-semantic result.",
        "Every comparable speed/throughput row in cpython_comparison_matrix must beat the CPython 3.12.11 p50; memory and binary-size budgets remain separate constraints.",
        "Slice 13's 20x comparison counts source-level awaits; the accepted fast path may eliminate repeated calls only after strict compile-time purity and dynamic callable/range guards, while impure/rebound fixtures must match CPython.",
        "Debug is measured for diagnosis; frozen measured thresholds apply to release.",
        "Creation includes close through the production positional-call/resume ABI; direct_resume is an isolated supplementary ABI measurement.",
        "Direct await and throughput run Python source compiled by the shipped CLI; timings include process startup, range iteration, and output, with no subtraction.",
        "The same bench_await_ready.py source is timed under CPython 3.12.11 with the identical whole-process scope; this is the fair language-level Rimera-vs-CPython comparison.",
        "verify_gate10.py itself is host-side CPython audit tooling, not a Gate 10 Rimera compilation fixture; compiling the verifier requires later language-conformance and stdlib/platform coverage.",
        "Timers use Compio and assert exactly two polls per root. Cancellation measures a real managed coroutine that catches the injected signal.",
        "Idle memory counts allocator-requested retained bytes on a fresh runtime heap, including task arena, runtime state, frames and root vectors; allocator metadata/OS pages are excluded.",
        "Source correctness, roots, race models and zero steady allocations are separately tested by the workspace suite.",
    ]
    return report


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    report = run()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
    print(json.dumps({"failures": report["failures"], "artifacts": report["artifacts"]}, indent=2))
    raise SystemExit(bool(report["failures"]))
