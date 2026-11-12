#!/usr/bin/env python3
"""Makes timed edits in an open editor through its MCP server, and dumps the
scene, for testing Team Create across machines (see docs/editor/team-create.md,
"Testing on two machines").

The editor must have the project open with Settings > AI connections >
Allow AI connections on: the script refuses to run without the project's
`.axcenity/mcp-endpoint.json`, because without it `--mcp` would open the
project without a window and edit the files instead of the session.

    # Every 15 s for 12 minutes: a Block named PC-001, PC-002…, a new colour
    # for the one made four edits before, and every third edit the other
    # machine's part (MAC-001…) made half transparent and renamed. Starts
    # when MAC-001 shows up; dumps the scene 90 s after the last edit and
    # again 90 s later (to pc-scene.txt and pc-scene.txt.later).
    python3 scripts/team-create-edits.py edit --editor EDITOR --project FOLDER \
        --prefix PC --other MAC --start-when MAC-001 --every 15 --for 720 \
        --out pc-scene.txt --log pc-edits.log

    # The Workspace's parts, sorted, one per line (to compare two machines).
    python3 scripts/team-create-edits.py dump --editor EDITOR --project FOLDER \
        --out pc-scene.txt

For a joining editor FOLDER is the session's files folder (the project the
editor has open). Standard library only (Python 3.8+).
"""

import argparse
import json
import os
import re
import subprocess
import sys
import time


class Mcp:
    """A JSON-RPC client over `EDITOR --mcp FOLDER`'s stdin and stdout."""

    def __init__(self, editor, project):
        self.process = subprocess.Popen(
            [editor, "--mcp", project, "--mode", "full"],
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=sys.stderr,
            text=True,
            encoding="utf-8",
            bufsize=1,
        )
        self.next_id = 0
        self.request(
            "initialize",
            {
                "protocolVersion": "2025-06-18",
                "capabilities": {},
                "clientInfo": {"name": "team-create-edits", "version": "1"},
            },
        )
        self.send({"jsonrpc": "2.0", "method": "notifications/initialized"})

    def send(self, message):
        self.process.stdin.write(json.dumps(message) + "\n")
        self.process.stdin.flush()

    def request(self, method, params):
        self.next_id += 1
        self.send({"jsonrpc": "2.0", "id": self.next_id, "method": method, "params": params})
        while True:
            line = self.process.stdout.readline()
            if not line:
                raise RuntimeError("the MCP server closed its output")
            message = json.loads(line)
            if message.get("id") == self.next_id:
                if "error" in message:
                    raise RuntimeError(f"{method}: {message['error']}")
                return message["result"]

    def call(self, tool, arguments):
        """The tool's text, and whether it was an error."""
        result = self.request("tools/call", {"name": tool, "arguments": arguments})
        text = "".join(part.get("text", "") for part in result.get("content", []))
        return text, bool(result.get("isError"))

    def close(self):
        self.process.stdin.close()
        self.process.wait(timeout=10)


def stamp():
    return time.strftime("%H:%M:%S")


def check_open(project):
    endpoint = os.path.join(project, ".axcenity", "mcp-endpoint.json")
    if not os.path.exists(endpoint):
        sys.exit(
            f"{endpoint} is missing: open the project in the editor and turn on "
            "Settings > AI connections > Allow AI connections first"
        )


def find(mcp, name):
    """The id of the part named exactly `name`, or None."""
    text, failed = mcp.call("scene_inspect", {"root": "Workspace", "name": name, "limit": 20})
    if failed:
        return None
    for line in text.splitlines()[1:]:
        words = line.split()
        if len(words) >= 3 and words[2] == f'"{name}"':
            return words[0]
    return None


def edit(args, mcp, log):
    if args.start_when:
        log(f"waiting for a part named {args.start_when}")
        while find(mcp, args.start_when) is None:
            time.sleep(2)
        log(f"{args.start_when} is here: starting")
    made = []
    start = time.monotonic()
    number = 0
    while time.monotonic() - start < args.duration:
        number += 1
        name = f"{args.prefix}-{number:03}"
        position = [args.x, 2 + (number % 5) * 2, number * 3 - 60]
        try:
            text, failed = mcp.call(
                "object_create", {"kind": "Block", "name": name, "position": position}
            )
            if not failed and text.startswith("created "):
                made.append(text.split()[1])
            log(f"create {name}: {'ERROR ' if failed else ''}{text}")
            if len(made) > 4:
                id = made[-5]
                color = [(number * 37 % 100) / 100, 0.5, 1 - (number * 37 % 100) / 100]
                text, failed = mcp.call("object_set", {"id": id, "props": {"Color": color}})
                log(f"recolour {id}: {'ERROR ' if failed else ''}{text}")
            if args.other and number % 3 == 0:
                # The same part as the other machine, so the two edits meet.
                other = f"{args.other}-{number // 3:03}"
                id = find(mcp, other)
                if id is not None:
                    text, failed = mcp.call(
                        "object_set", {"id": id, "props": {"Transparency": 0.5}, "name": other + "-seen"}
                    )
                    log(f"touch {other}: {'ERROR ' if failed else ''}{text}")
        except Exception as error:  # keep going: a cut is the point of the test
            log(f"call failed: {error}")
        time.sleep(max(0.0, start + number * args.every - time.monotonic()))
    log(f"done: {number} creates")
    if args.out:
        for round in (1, 2):
            log(f"waiting {args.settle:.0f} s for the session to settle")
            time.sleep(args.settle)
            out = args.out if round == 1 else args.out + ".later"
            dump_to(mcp, out, log)


def dump(args, mcp, log):
    dump_to(mcp, args.out, log)


def dump_to(mcp, out_path, log):
    """Every part in the Workspace with its name, position, size, colour and
    transparency, sorted by name. Ids are per editor, so they're left out."""
    text, failed = mcp.call(
        "scene_inspect", {"root": "Workspace", "class": "BasePart", "limit": 500}
    )
    ids = [line.split()[0] for line in text.splitlines()[1:] if line.strip()[:1].isdigit()]
    rows = []
    for id in ids:
        detail, _ = mcp.call(
            "object_get",
            {"id": id, "props": ["Name", "Position", "Size", "Color", "Transparency"]},
        )
        row = " | ".join(part.strip() for part in detail.replace(id, "#").splitlines())
        rows.append(re.sub(r" in [0-9.]+ ·", " ·", row))
    rows.sort(key=lambda row: row.split('"')[1] if '"' in row else row)
    with open(out_path, "w", encoding="utf-8") as out:
        out.write("\n".join(rows) + "\n")
    log(f"{len(rows)} parts written to {out_path}{' (scene_inspect ERROR)' if failed else ''}")


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("what", choices=["edit", "dump"])
    parser.add_argument("--editor", required=True, help="the axcenity-editor binary")
    parser.add_argument("--project", required=True, help="the folder the editor has open")
    parser.add_argument("--prefix", default="Edit", help="names of the parts it makes")
    parser.add_argument("--every", type=float, default=15.0, help="seconds between edits")
    parser.add_argument("--for", dest="duration", type=float, default=720.0, help="seconds")
    parser.add_argument("--x", type=float, default=0.0, help="the parts' X position")
    parser.add_argument("--out", help="dump: the file; edit: dump here when done")
    parser.add_argument("--settle", type=float, default=90.0, help="s before the dump")
    parser.add_argument("--start-when", help="edit: wait for a part with this name")
    parser.add_argument("--other", help="edit: the other machine's prefix, to touch its parts")
    parser.add_argument("--log")
    parser.add_argument(
        "--no-window-ok", action="store_true", help="for trying the script: edit the files"
    )
    args = parser.parse_args()
    if not args.no_window_ok:
        check_open(args.project)
    log_file = open(args.log, "a", encoding="utf-8") if args.log else None

    def log(text):
        line = f"{stamp()} {text}"
        print(line, flush=True)
        if log_file:
            log_file.write(line + "\n")
            log_file.flush()

    mcp = Mcp(args.editor, args.project)
    try:
        (edit if args.what == "edit" else dump)(args, mcp, log)
    finally:
        mcp.close()


if __name__ == "__main__":
    main()
