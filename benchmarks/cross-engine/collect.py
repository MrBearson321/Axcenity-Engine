#!/usr/bin/env python3
"""Collects XB10k results from engine logs into one table.

    python collect.py <log or json file>...

Reads `XB10K_RESULT {json}` lines (Godot, Unity, Unreal, s&box, Roblox) and
Axcenity's `--bench` JSON line (`{"host": ...}`), and prints a Markdown table
of the median of each engine's runs.
"""
import json
import statistics
import sys


def parse(path):
    with open(path, encoding="utf-8", errors="replace") as f:
        for line in f:
            if "XB10K_RESULT " in line:
                yield json.loads(line.split("XB10K_RESULT ", 1)[1].strip())
            elif line.startswith('{"host"'):
                data = json.loads(line)
                frame = next(s for s in data["stages"] if s["name"] == "frame")
                yield {
                    "engine": "Axcenity", "version": "main", "renderer": f"wgpu ({data['host']})",
                    "frames": data["frames"], "median_ms": frame["median_ms"], "mean_ms": frame["mean_ms"],
                    "p95_ms": frame["p95_ms"], "worst_ms": frame["max_ms"],
                    "first_frame_ms": data.get("startup", {}).get("first frame"),
                    "fixed_timestep": True,
                }


def main(paths):
    runs = {}
    for path in paths:
        for result in parse(path):
            key = (result["engine"], result.get("renderer", ""))
            runs.setdefault(key, []).append(result)
    print("| Engine | Renderer | Runs | Median frame (ms) | FPS | p95 (ms) | Worst (ms) | Fixed step |")
    print("| --- | --- | --- | --- | --- | --- | --- | --- |")
    for (engine, renderer), results in sorted(runs.items(), key=lambda kv: statistics.median(r["median_ms"] for r in kv[1])):
        median = statistics.median(r["median_ms"] for r in results)
        p95 = statistics.median(r["p95_ms"] for r in results)
        worst = max(r["worst_ms"] for r in results)
        fixed = "yes" if all(r.get("fixed_timestep") for r in results) else "no"
        print(f"| {engine} {results[0].get('version', '')} | {renderer} | {len(results)} | {median:.2f} | {1000 / median:.0f} | {p95:.2f} | {worst:.1f} | {fixed} |")


if __name__ == "__main__":
    main(sys.argv[1:])
