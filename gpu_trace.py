#!/usr/bin/env python3
"""Autonomous GPU profiling via Nsight Graphics' CLI (ngfx.exe).

Captures a GPU Trace of this repo's SDF pipeline and prints a compact,
parseable summary: per-pass GPU times (from wgpu debug labels) and the
hardware counters that say WHY a pass is slow (SM/L2/L1TEX throughput,
top warp-stall reasons).

Modes:
  probe (default)   Headless: traces the `gpu_probe_loop` ignored test in
                    iced_nodegraph_sdf (release + WGPU_DEBUG=1, so the
                    "sdf_index" / "sdf_shade" pass labels reach the driver
                    without validation-layer overhead). Per-pass timing and
                    counters. Trigger: submit count, no window needed.
  --demo NAME       Whole-app: traces a demo binary (e.g. 500_nodes) after a
                    frame warmup. Frame-level GPU time and counters only:
                    iced_wgpu hardcodes InstanceFlags::empty(), so wgpu debug
                    labels never reach the driver and passes cannot be
                    attributed by name in this mode.

The raw .ngfx-gputrace report is kept next to the exports for deep dives in
the Nsight Graphics GUI (shader profiler, SASS, per-line stalls).

Requires: Nsight Graphics >= 2023 (auto-detected under Program Files, newest
version wins; override with --ngfx or the NGFX env var). GPU clocks are
locked to base during the trace, so numbers are stable run-to-run but ~15%
below boost-clock wall time.
"""

import argparse
import csv
import glob
import json
import math
import statistics
import subprocess
import sys
import time
from collections import defaultdict
from pathlib import Path

REPO = Path(__file__).resolve().parent

# Regime metrics summarized per labeled pass (median across traced instances).
KEY_METRICS = [
    ("SM busy %", "GPUTrace.sm__throughput.avg.pct_of_peak_sustained_elapsed"),
    ("L1TEX %", "GPUTrace.l1tex__throughput.avg.pct_of_peak_sustained_elapsed"),
    ("L2 %", "GPUTrace.lts__throughput.avg.pct_of_peak_sustained_elapsed"),
    ("DRAM %", "dram__sectors.avg.pct_of_peak_sustained_elapsed"),
    ("PS warps/cyc", "GPUTrace.PCSampler.tpc__warps_active_shader_pixel.avg.per_cycle_elapsed"),
    ("CS warps/cyc", "GPUTrace.PCSampler.tpc__warps_active_shader_compute.avg.per_cycle_elapsed"),
]
STALL_PREFIX = "GPUTrace.PCSampler.tpc__warps_issue_stalled_"


def find_ngfx(override: str | None) -> Path:
    if override:
        return Path(override)
    hits = glob.glob(
        r"C:\Program Files\NVIDIA Corporation\Nsight Graphics *"
        r"\host\windows-desktop-nomad-x64\ngfx.exe"
    )
    if not hits:
        sys.exit("ngfx.exe not found; install Nsight Graphics or pass --ngfx <path>")
    return Path(sorted(hits)[-1])  # lexicographic = newest version


def build_probe_exe() -> Path:
    """Build the release test binary of iced_nodegraph_sdf, return its path."""
    out = subprocess.run(
        ["cargo", "test", "-p", "iced_nodegraph_sdf", "--release",
         "--no-run", "--message-format=json"],
        cwd=REPO, capture_output=True, text=True, check=True,
    )
    for line in out.stdout.splitlines():
        try:
            msg = json.loads(line)
        except json.JSONDecodeError:
            continue
        target = msg.get("target") or {}
        if msg.get("executable") and target.get("name") == "iced_nodegraph_sdf":
            return Path(msg["executable"])
    sys.exit("could not locate iced_nodegraph_sdf test executable in cargo output")


def build_demo_exe(name: str) -> Path:
    subprocess.run(
        ["cargo", "build", "--release", "-p", f"demo_{name}", "--bin", name],
        cwd=REPO, check=True,
    )
    return REPO / "target" / "release" / f"{name}.exe"


def run_capture(ngfx: Path, exe: Path, out_dir: Path, *, args: str = "",
                env: str = "", triggers: list[str]) -> None:
    out_dir.mkdir(parents=True, exist_ok=True)
    cmd = [
        str(ngfx),
        "--activity", "GPU Trace Profiler",
        "--exe", str(exe),
        "--dir", str(REPO),
        "--output-dir", str(out_dir),
        "--auto-export",
        *triggers,
    ]
    if args:
        cmd += ["--args", args]
    if env:
        cmd += ["--env", env]
    res = subprocess.run(cmd, capture_output=True, text=True, timeout=600)
    log = res.stdout + res.stderr
    (out_dir / "ngfx.log").write_text(log, encoding="utf-8")
    if res.returncode != 0 or "Succeeded to export data" not in log:
        sys.exit(f"capture failed (exit {res.returncode}); see {out_dir / 'ngfx.log'}")


def read_tsv(path: Path) -> list[list[str]]:
    with open(path, newline="", encoding="utf-8", errors="replace") as f:
        return list(csv.reader(f, delimiter="\t"))


def parse_frame_times(base: Path) -> list[float]:
    rows = read_tsv(base / "FRAME.xls")
    for row in rows:
        if row and row[0] == "GPU frame time":
            return [float(v) for v in row[1:] if v]
    return []


def parse_markers(base: Path) -> dict[str, list[float]]:
    rows = read_tsv(base / "D3DPERF_EVENTS.xls")
    events: dict[str, list[float]] = defaultdict(list)
    for row in rows[1:]:
        if len(row) >= 2 and row[1]:
            events[row[0]].append(float(row[1]))
    return events


def parse_regimes(base: Path) -> dict[str, dict[str, list[float]]]:
    """event name -> metric name -> values (one per traced pass instance)."""
    rows = read_tsv(base / "GPUTRACE_REGIMES.xls")
    if len(rows) < 2:
        return {}
    header = rows[0]
    events: dict[str, dict[str, list[float]]] = defaultdict(lambda: defaultdict(list))
    for row in rows[1:]:
        metrics = events[row[0]]
        for name, value in zip(header[1:], row[1:]):
            if value and value != "N/A":
                try:
                    metrics[name].append(float(value))
                except ValueError:
                    pass
    return events


def median(values: list[float]) -> float:
    return statistics.median(values) if values else math.nan


def report(out_dir: Path, mode: str) -> None:
    base = out_dir / "BASE"
    print(f"\n=== GPU trace summary ({mode}) ===")
    print(f"raw report: {next(out_dir.glob('*.ngfx-gputrace'), '?')}")

    frames = parse_frame_times(base)
    # In probe mode there are no presents; the trace is one giant "frame"
    # whose duration is meaningless. Only report when several frames exist.
    if len(frames) > 1:
        print(f"\nGPU frame time: median {median(frames):.3f} ms "
              f"(min {min(frames):.3f}, max {max(frames):.3f}, n={len(frames)})")

    markers = parse_markers(base)
    regimes = parse_regimes(base)
    if not markers:
        print("\nno per-pass markers in this capture "
              "(expected for --demo mode: iced strips wgpu debug labels)")
        return

    print(f"\n{'pass':12s} {'median ms':>10s} {'min ms':>8s} {'max ms':>8s} {'n':>4s}")
    for name, times in sorted(markers.items()):
        print(f"{name:12s} {median(times):10.3f} {min(times):8.3f} "
              f"{max(times):8.3f} {len(times):4d}")

    for name in sorted(regimes):
        metrics = regimes[name]
        print(f"\n[{name}]")
        for label, key in KEY_METRICS:
            value = median(metrics.get(key, []))
            if not math.isnan(value):
                print(f"  {label:14s} {value:7.2f}")
        stalls = sorted(
            ((median(vals), key[len(STALL_PREFIX):].split(".")[0])
             for key, vals in metrics.items()
             if key.startswith(STALL_PREFIX) and key.endswith("pct_of_peak_sustained_elapsed")),
            reverse=True,
        )[:4]
        line = ", ".join(f"{stall} {value:.1f}" for value, stall in stalls
                         if not math.isnan(value) and value >= 0.5)
        if line:
            print(f"  top stalls (% of peak): {line}")


def main() -> None:
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--demo", metavar="NAME",
                        help="trace a demo binary (e.g. 500_nodes) instead of the headless probe")
    parser.add_argument("--warmup", type=int, default=None,
                        help="submits (probe) or frames (demo) to skip before tracing "
                             "(default: 100 submits / 120 frames)")
    parser.add_argument("--span", type=int, default=None,
                        help="submits (probe) or frames (demo) to trace "
                             "(default: 30 submits / 5 frames)")
    parser.add_argument("--out", type=Path, default=None,
                        help="output dir (default target/gpu_trace/<timestamp>)")
    parser.add_argument("--ngfx", default=None, help="path to ngfx.exe")
    opts = parser.parse_args()

    ngfx = find_ngfx(opts.ngfx)
    stamp = time.strftime("%Y%m%d_%H%M%S")
    out_dir = opts.out or REPO / "target" / "gpu_trace" / stamp

    if opts.demo:
        exe = build_demo_exe(opts.demo)
        triggers = [
            "--start-after-frames", str(opts.warmup if opts.warmup is not None else 120),
            "--limit-to-frames", str(opts.span if opts.span is not None else 5),
        ]
        run_capture(ngfx, exe, out_dir, triggers=triggers)
        report(out_dir, f"demo {opts.demo}")
    else:
        exe = build_probe_exe()
        triggers = [
            "--start-after-submits", str(opts.warmup if opts.warmup is not None else 100),
            "--limit-to-submits", str(opts.span if opts.span is not None else 30),
        ]
        run_capture(
            ngfx, exe, out_dir,
            args="gpu_probe_loop --ignored --nocapture --test-threads=1",
            env="WGPU_DEBUG=1; GPU_PROBE_SECS=120;",
            triggers=triggers,
        )
        report(out_dir, "headless probe")


if __name__ == "__main__":
    main()
