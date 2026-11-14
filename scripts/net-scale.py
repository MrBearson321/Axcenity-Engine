#!/usr/bin/env python3
"""Multiplayer scale runs (G13, multiplayer readiness MP1).

Runs `axcenity-server` and `axcenity-bots` (build them first:
`cargo build --release -p axcenity-server --bins`) and writes a report.

One machine, server and bots:
  python scripts/net-scale.py run examples/hello-spawn --bots 32 --seconds 120

Two machines (the server and some bots on one, more bots on the other):
  on the host:    python scripts/net-scale.py run <game> --bots 16 --seconds 300 --port 47800
  on the other:   python scripts/net-scale.py join <game> 192.168.1.192:47800 --bots 16 --seconds 280
  then:           python scripts/net-scale.py report <host's run folder> <other's run folder>

Each run writes a folder under target/net-scale/ (or --out) with the
server's and bots' JSON lines (server.jsonl, bots-N.jsonl), their console
output, and report.md. `report` reads several such folders into one report.

Standard library only; works on Windows, macOS and Linux.
"""

from __future__ import annotations

import argparse
import datetime
import json
import os
import platform
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
EXE = ".exe" if os.name == "nt" else ""


def binary(bin_dir: Path, name: str) -> str:
    path = bin_dir / f"{name}{EXE}"
    if not path.exists():
        sys.exit(f"{path} is missing: cargo build --release -p axcenity-server --bins")
    return str(path)


def run_folder(out: Path | None, label: str) -> Path:
    stamp = datetime.datetime.now().strftime("%Y%m%d-%H%M%S")
    folder = (out or ROOT / "target" / "net-scale") / f"{stamp}-{label}-{platform.node()}"
    folder.mkdir(parents=True, exist_ok=True)
    return folder


def split(total: int, parts: int) -> list[int]:
    parts = max(1, min(parts, total))
    return [total // parts + (1 if i < total % parts else 0) for i in range(parts)]


def start_bots(args, folder: Path, address: str) -> list[subprocess.Popen]:
    procs = []
    for n, count in enumerate(split(args.bots, args.bot_processes)):
        command = [
            binary(args.bin_dir, "axcenity-bots"), str(args.game), "--join", address,
            "--count", str(count), "--seconds", str(args.bot_seconds or args.seconds),
            "--report", str(args.report), "--report-file", str(folder / f"bots-{n}.jsonl"),
            # Distinct UserIds across processes and machines.
            "--user-id", str(args.user_id_base + n * 1000),
        ]
        if args.no_scripts:
            command.append("--no-scripts")
        if args.idle:
            command.append("--idle")
        log = open(folder / f"bots-{n}.log", "w")
        procs.append(subprocess.Popen(command, stdout=log, stderr=subprocess.STDOUT))
        time.sleep(0.2)
    return procs


def command_run(args) -> None:
    folder = run_folder(args.out, "run")
    server_log = open(folder / "server.log", "w")
    server = subprocess.Popen(
        [
            binary(args.bin_dir, "axcenity-server"), str(args.game), "--port", str(args.port),
            "--max-players", str(args.max_players or max(args.bots, 8) + 16),
            "--stats", str(args.report), "--stats-file", str(folder / "server.jsonl"),
        ],
        stdout=server_log,
        stderr=subprocess.STDOUT,
    )
    time.sleep(args.server_warm_up)
    procs = start_bots(args, folder, f"127.0.0.1:{args.port}") if args.bots > 0 else []
    try:
        deadline = time.time() + args.seconds + 15
        for proc in procs:
            proc.wait(timeout=max(1, deadline - time.time()))
        if not procs:
            server.wait(timeout=args.seconds)
    except subprocess.TimeoutExpired:
        pass
    finally:
        for proc in procs:
            proc.kill() if proc.poll() is None else None
        server.terminate()
        try:
            server.wait(timeout=5)
        except subprocess.TimeoutExpired:
            server.kill()
    write_report([folder], folder / "report.md", args)
    print((folder / "report.md").read_text())
    print(f"Run folder: {folder}")


def command_join(args) -> None:
    folder = run_folder(args.out, "join")
    procs = start_bots(args, folder, args.address)
    for proc in procs:
        proc.wait()
    write_report([folder], folder / "report.md", args)
    print((folder / "report.md").read_text())
    print(f"Run folder: {folder}")


def read_jsonl(path: Path) -> list[dict]:
    lines = []
    for line in path.read_text().splitlines():
        try:
            lines.append(json.loads(line))
        except json.JSONDecodeError:
            pass
    return lines


def write_report(folders: list[Path], out: Path, args=None) -> None:
    rows = ["# Multiplayer scale run", ""]
    if args is not None and hasattr(args, "game"):
        rows.append(f"Game `{args.game}`, {args.bots} bots, {args.seconds} s, on {platform.node()} ({platform.system()}).")
        rows.append("")
    for folder in folders:
        server = folder / "server.jsonl"
        if server.exists():
            lines = read_jsonl(server)
            busy = [line for line in lines if line.get("players", 0) > 0]
            rows += [f"## Server ({folder.name})", ""]
            if busy:
                peak = max(line["players"] for line in busy)
                full = [line for line in busy if line["players"] == peak]
                p95 = max(line["frame_p95_ms"] for line in full)
                worst = max(line["frame_max_ms"] for line in full)
                fps = min(line["fps"] for line in full)
                sent = sum(line["sent_bytes_per_s"] for line in full) / len(full)
                rtt = max(line["rtt_max_ms"] for line in full)
                rows += [
                    f"- Peak players: **{peak}** ({len(full)} reports at that count)",
                    f"- Frames a second at peak (lowest report): {fps:.0f}",
                    f"- Frame time at peak: worst p95 {p95:.2f} ms, worst single frame {worst:.2f} ms",
                    f"- Sent at peak: {sent / 1000:.1f} KB/s ({sent / 1000 / peak:.1f} KB/s per player)",
                    f"- Highest round trip at peak: {rtt:.0f} ms",
                    "",
                ]
            else:
                rows += ["No players joined.", ""]
        for bots in sorted(folder.glob("bots-*.jsonl")):
            lines = read_jsonl(bots)
            summary = next((line for line in reversed(lines) if line.get("summary")), None)
            rows += [f"## Bots {bots.stem} ({folder.name})", ""]
            if summary is None:
                rows += ["No summary (the process didn't finish).", ""]
                continue
            rows += [
                f"- {summary['joined']} of {summary['bots']} joined; join p50 {summary['join_p50_s']:.2f} s, max {summary['join_max_s']:.2f} s",
                f"- Connected at the last report: {summary['connected']}",
                f"- Round trip p50 {summary['rtt_p50_ms']:.0f} ms, p95 {summary['rtt_p95_ms']:.0f}, max {summary['rtt_max_ms']:.0f}; loss {summary['loss_mean'] * 100:.1f}%",
                f"- Each bot sends {summary['sent_bytes_per_s'] / 1000:.1f} KB/s and receives {summary['received_bytes_per_s'] / 1000:.1f} KB/s",
                f"- Corrections (server moved a character back): {summary['corrections']}; script errors: {summary['script_errors']}",
                f"- Bots' own frames: mean {summary['bot_frame_ms_mean']:.2f} ms, worst {summary['bot_frame_ms_max']:.2f} ms",
            ]
            for reason, count in summary.get("ended", {}).items():
                rows.append(f"- {count} ended: {reason}")
            rows.append("")
    out.write_text("\n".join(rows))


def command_report(args) -> None:
    out = args.out or Path(args.folders[0]) / "combined-report.md"
    write_report([Path(f) for f in args.folders], out)
    print(out.read_text())


def main() -> None:
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest="command", required=True)

    def common(p):
        p.add_argument("game", type=Path, help="project folder (the same game on every machine)")
        p.add_argument("--bots", type=int, default=16)
        p.add_argument("--bot-processes", type=int, default=2)
        p.add_argument("--seconds", type=float, default=120)
        p.add_argument("--bot-seconds", type=float, default=0, help="how long bots stay (default --seconds)")
        p.add_argument("--report", type=float, default=5)
        p.add_argument("--no-scripts", action="store_true")
        p.add_argument("--idle", action="store_true")
        p.add_argument("--user-id-base", type=int, default=2_000_000)
        p.add_argument("--bin-dir", type=Path, default=ROOT / "target" / "release")
        p.add_argument("--out", type=Path)

    run = sub.add_parser("run", help="host with axcenity-server here and join bots to it")
    common(run)
    run.add_argument("--port", type=int, default=47800)
    run.add_argument("--max-players", type=int, default=0)
    run.add_argument("--server-warm-up", type=float, default=3)
    run.set_defaults(func=command_run)

    join = sub.add_parser("join", help="join bots to a server elsewhere")
    common(join)
    join.add_argument("address", help="the server's address:port")
    join.set_defaults(func=command_join)

    report = sub.add_parser("report", help="one report from several run folders")
    report.add_argument("folders", nargs="+")
    report.add_argument("--out", type=Path)
    report.set_defaults(func=command_report)

    args = parser.parse_args()
    args.func(args)


if __name__ == "__main__":
    main()
