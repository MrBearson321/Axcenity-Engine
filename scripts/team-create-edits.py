#!/usr/bin/env python3
"""Makes timed edits in an open editor through its MCP server, and dumps the
scene, for testing Team Create across machines (see docs/editor/team-create.md,
"Testing on two machines").

The editor must have the project open with Settings > AI connections >
Allow AI connections on: the script refuses to run without the project's
`.axcenity/mcp-endpoint.json`, because without it `--mcp` would open the
project without a window and edit the files instead of the session.

    # Every 15 s for 12 minutes: a Block named PC-001, PC-002…, and a new
    # colour for the one made four edits before.
    python3 scripts/team-create-edits.py edit --editor EDITOR --project FOLDER \
        --prefix PC --every 15 --for 720 --log pc-edits.log

    # The Workspace's parts, sorted, one per line (to compare two machines).
    python3 scripts/team-create-edits.py dump --editor EDITOR --project FOLDER \
        --out pc-scene.txt

For a joining editor FOLDER is the session's files folder (the project the
editor has open). Standard library only (Python 3.8+).
"""

import argparse
import json
import os
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


def edit(args, mcp, log):
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
        except Exception as error:  # keep going: a cut is the point of the test
            log(f"call failed: {error}")
        time.sleep(max(0.0, start + number * args.every - time.monotonic()))
    log(f"done: {number} creates")


def dump(args, mcp, log):
    text, failed = mcp.call(
        "scene_inspect", {"root": "Workspace", "class": "BasePart", "limit": 500}
    )
    lines = sorted(line.strip() for line in text.splitlines()[1:] if line.strip())
    # Ids are per editor (session-scoped), so compare names and paths only.
    names = sorted(line.split(" ", 1)[1] if line[:1].isdigit() else line for line in lines)
    with open(args.out, "w", encoding="utf-8") as out:
        out.write("\n".join(names) + "\n")
    log(f"{len(names)} parts written to {args.out}{' (ERROR)' if failed else ''}")


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("what", choices=["edit", "dump"])
    parser.add_argument("--editor", required=True, help="the axcenity-editor binary")
    parser.add_argument("--project", required=True, help="the folder the editor has open")
    parser.add_argument("--prefix", default="Edit", help="names of the parts it makes")
    parser.add_argument("--every", type=float, default=15.0, help="seconds between edits")
    parser.add_argument("--for", dest="duration", type=float, default=720.0, help="seconds")
    parser.add_argument("--x", type=float, default=0.0, help="the parts' X position")
    parser.add_argument("--out", default="scene.txt")
    parser.add_argument("--log")
    args = parser.parse_args()
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
