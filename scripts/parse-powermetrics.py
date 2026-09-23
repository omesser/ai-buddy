#!/usr/bin/env python3
"""Reduce a `powermetrics --samplers tasks,cpu_power` capture (see
scripts/bench-wakeups-macos.sh) to the numbers #431 asks for: wakeups/sec for
a named process, package idle residency, and CPU power. Optionally splits by
what the app's frame log says was on screen each second, so one capture can
answer more than one #423 scenario when a Behavior changes naturally during
the sample.

Usage: parse-powermetrics.py POWERMETRICS_TXT [--process NAME] [--frame-log PATH]
"""
import argparse
import re
import statistics
import sys
from datetime import datetime, timezone

SAMPLE_RE = re.compile(
    r"\*\*\* Sampled system activity \((.+?)\) \((\d+(?:\.\d+)?)ms elapsed\)"
)
# Idle-family animations, per characters/cat/character.manifest's [animations.*].
IDLE_ANIMS = {"idle", "waiting", "sit", "sleep"}


def parse_samples(text, process, pid=None):
    """One dict per '*** Sampled system activity' block.

    Matching by process name alone is unsafe: other agents on this machine
    run their own ai-buddy builds concurrently (observed directly - a second,
    unrelated `ai-buddy` PID showed up in a real capture during this task).
    Pass --pid for the exact process this script launched; name matching is
    a fallback for ad-hoc use only and prints a warning if more than one PID
    answers to it in any sample.
    """
    blocks = re.split(r"(?=\*\*\* Sampled system activity)", text)
    samples = []
    ambiguous_warned = False
    for block in blocks:
        m = SAMPLE_RE.search(block)
        if not m:
            continue
        # powermetrics prints local time with a zone offset it does not name
        # portably; strptime with %z handles "+0300" but not "+03:00", so
        # normalize.
        ts_raw = re.sub(r"([+-]\d{2}):?(\d{2})$", r"\1\2", m.group(1))
        try:
            ts = datetime.strptime(ts_raw, "%a %b %d %H:%M:%S %Y %z")
        except ValueError:
            ts = None
        elapsed_ms = float(m.group(2))

        candidates = []
        for line in block.splitlines() if process else ():
            cols = line.split()
            if len(cols) >= 7 and process in line and not line.startswith("Name"):
                if pid is not None:
                    if cols[1] == str(pid):
                        candidates.append(line)
                else:
                    candidates.append(line)
        if pid is None and len(candidates) > 1 and not ambiguous_warned:
            print(
                f"warning: {len(candidates)} processes match name {process!r} "
                "in at least one sample - pass --pid to disambiguate",
                file=sys.stderr,
            )
            ambiguous_warned = True
        proc_line = candidates[0] if candidates else None
        wakeups_intr = wakeups_pkgidle = cpu_ms_s = None
        if proc_line is not None:
            # Columns: Name... ID CPU-ms/s User% Deadlines(<2,2-5) Wakeups(Intr,PkgIdle)
            nums = re.findall(r"-?\d+\.\d+|-?\d+", proc_line)
            if len(nums) >= 6:
                tail = nums[-6:]
                cpu_ms_s = float(tail[0])
                wakeups_intr = float(tail[-2])
                wakeups_pkgidle = float(tail[-1])

        e_idle = re.search(r"E-Cluster idle residency:\s*([\d.]+)%", block)
        p_idle = re.search(r"P-Cluster idle residency:\s*([\d.]+)%", block)
        pkg_power = re.search(r"CPU Power:\s*(\d+)\s*mW", block)

        samples.append(
            {
                "ts": ts,
                "elapsed_ms": elapsed_ms,
                "wakeups_intr": wakeups_intr,
                "wakeups_pkgidle": wakeups_pkgidle,
                "cpu_ms_s": cpu_ms_s,
                "e_idle_pct": float(e_idle.group(1)) if e_idle else None,
                "p_idle_pct": float(p_idle.group(1)) if p_idle else None,
                "pkg_power_mw": float(pkg_power.group(1)) if pkg_power else None,
            }
        )
    return samples


def load_frame_timeline(path):
    """[(unix_seconds, animation)] from `frame: <ms> ... <anim>#<idx> <id>` lines."""
    timeline = []
    line_re = re.compile(r"^frame: (\d+) .*?\s([A-Za-z_-]+)#\d+\s")
    with open(path) as f:
        for line in f:
            m = line_re.match(line)
            if m:
                timeline.append((int(m.group(1)) // 1000, m.group(2)))
    return timeline


def animation_at(timeline, unix_seconds):
    if not timeline or unix_seconds is None:
        return None
    best = None
    for ts, anim in timeline:
        if ts <= unix_seconds:
            best = anim
        else:
            break
    return best


def category(anim):
    if anim is None:
        return "unknown"
    return "idle" if anim in IDLE_ANIMS else "active"


def summarize(samples, label):
    def avg(key):
        vals = [s[key] for s in samples if s[key] is not None]
        return statistics.mean(vals) if vals else None

    n = len(samples)
    print(f"-- {label} (n={n} samples) --")
    if n == 0:
        print("  no samples")
        return
    wi, wp, cpu, e, p, pw = (
        avg("wakeups_intr"),
        avg("wakeups_pkgidle"),
        avg("cpu_ms_s"),
        avg("e_idle_pct"),
        avg("p_idle_pct"),
        avg("pkg_power_mw"),
    )
    if wi is not None:
        print(f"  wakeups/sec (interrupt):      {wi:.2f}")
    elif label.strip().endswith("()"):
        print("  (no process of interest: system-wide numbers only)")
    else:
        print("  wakeups/sec (interrupt):      process not found in any sample")
    if wp is not None:
        print(f"  wakeups/sec (pulled pkg idle):{wp:.2f}")
    if cpu is not None:
        print(f"  CPU%:                         {cpu / 10:.2f}")
    if e is not None:
        print(f"  E-Cluster idle residency avg: {e:.1f}%")
    if p is not None:
        print(f"  P-Cluster idle residency avg: {p:.1f}%")
    if pw is not None:
        print(f"  Package CPU power avg:        {pw:.0f} mW")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("powermetrics_txt")
    ap.add_argument(
        "--process",
        default="ai-buddy",
        help="process name to reduce; pass '' for a baseline capture with no process of interest",
    )
    ap.add_argument("--pid", type=int, default=None, help="exact PID to match (recommended)")
    ap.add_argument("--frame-log", default=None)
    args = ap.parse_args()

    text = open(args.powermetrics_txt, errors="replace").read()
    samples = parse_samples(text, args.process, args.pid)
    if not samples:
        print("no '*** Sampled system activity' blocks found", file=sys.stderr)
        sys.exit(1)

    summarize(samples, f"overall ({args.process})")

    if args.frame_log:
        timeline = load_frame_timeline(args.frame_log)
        if not timeline:
            print(
                f"\n(frame log {args.frame_log} had no matching 'frame:' lines - "
                "was AI_BUDDY_TRACE_FRAMES=1 set?)",
                file=sys.stderr,
            )
            return
        buckets = {"idle": [], "active": [], "unknown": []}
        by_anim = {}
        anims_seen = set()
        for s in samples:
            unix_s = int(s["ts"].timestamp()) if s["ts"] else None
            anim = animation_at(timeline, unix_s)
            if anim:
                anims_seen.add(anim)
            buckets[category(anim)].append(s)
            by_anim.setdefault(anim, []).append(s)
        print(f"\nanimations observed in frame log during capture: {sorted(anims_seen) or 'none'}")
        for label, bucket in buckets.items():
            if bucket:
                print()
                summarize(bucket, f"{label}-animation seconds ({args.process})")
        print("\nper-animation breakdown:")
        for anim, bucket in sorted(by_anim.items(), key=lambda kv: -len(kv[1])):
            print()
            summarize(bucket, f"anim={anim} ({args.process})")


if __name__ == "__main__":
    main()
