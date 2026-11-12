#!/usr/bin/env python3
"""A TCP relay that can cut the network for a while, for testing Team Create
(or multiplayer) across two machines without touching the firewall.

Run it on the joining machine and point the joiner at it:

    python3 scripts/net-cut-relay.py --listen 47899 --target 192.168.1.20:47810 \
        --cut-after 120 --cut-for 300

then change the invite link's `host=` to `127.0.0.1:47899` (the certificate
fingerprint in the link still pins the real host, so TLS checks it as usual).

While the cut lasts the relay behaves like a dead link, not like a closed
socket: bytes in both directions are held (TCP would retransmit them), a
close from either end is not passed on, and new connections are accepted but
not connected onwards (the client's handshake stalls, as a connect over a dead
link does). When the cut ends, held bytes and closes go through and stalled
connections are connected to the target. A client that gave up during the cut
has closed its side, so its connection just ends.

`--cut-now-file PATH` adds manual control: the link is cut while PATH exists.
Every event is printed with the time, to stdout and to `--log` if given.
Standard library only (Python 3.8+).
"""

import argparse
import asyncio
import os
import sys
import time

START = time.monotonic()
LOG = None


def say(text):
    line = f"{time.strftime('%H:%M:%S')} (+{time.monotonic() - START:7.1f} s) {text}"
    print(line, flush=True)
    if LOG:
        LOG.write(line + "\n")
        LOG.flush()


class Link:
    """Whether the network is up, from the timer and the manual file."""

    def __init__(self, args):
        self.args = args
        self.changed = asyncio.Event()
        self.was_cut = False

    def cut(self):
        elapsed = time.monotonic() - START
        timed = (
            self.args.cut_after is not None
            and self.args.cut_after <= elapsed < self.args.cut_after + self.args.cut_for
        )
        manual = self.args.cut_now_file is not None and os.path.exists(self.args.cut_now_file)
        return timed or manual

    async def watch(self):
        while True:
            now = self.cut()
            if now != self.was_cut:
                self.was_cut = now
                say("LINK CUT" if now else "LINK RESTORED")
                self.changed.set()
                self.changed = asyncio.Event()
            await asyncio.sleep(0.1)

    async def up(self):
        while self.cut():
            await self.changed.wait()


async def pump(name, reader, writer, link, stats):
    """Copies one direction; holds bytes and the close while the link is cut."""
    try:
        while True:
            data = await reader.read(65536)
            await link.up()
            if not data:
                break
            writer.write(data)
            stats[name] += len(data)
            await writer.drain()
    except (ConnectionError, OSError) as error:
        say(f"  {name}: {error}")
    finally:
        await link.up()
        try:
            writer.close()
        except Exception:
            pass


async def serve(client_reader, client_writer, link, target, counter):
    counter[0] += 1
    number = counter[0]
    peer = client_writer.get_extra_info("peername")
    say(f"#{number} connection from {peer}" + (" (held: link is cut)" if link.cut() else ""))
    await link.up()
    host, port = target
    try:
        upstream_reader, upstream_writer = await asyncio.open_connection(host, port)
    except OSError as error:
        say(f"#{number} couldn't reach {host}:{port}: {error}")
        client_writer.close()
        return
    stats = {"to host": 0, "to client": 0}
    await asyncio.gather(
        pump("to host", client_reader, upstream_writer, link, stats),
        pump("to client", upstream_reader, client_writer, link, stats),
    )
    say(f"#{number} ended ({stats['to host']} B to host, {stats['to client']} B to client)")


async def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--listen", type=int, required=True, help="local port")
    parser.add_argument("--bind", default="127.0.0.1")
    parser.add_argument("--target", required=True, help="host:port to relay to")
    parser.add_argument("--cut-after", type=float, help="seconds before the cut")
    parser.add_argument("--cut-for", type=float, default=300.0, help="cut length, s")
    parser.add_argument("--cut-now-file", help="the link is cut while this file exists")
    parser.add_argument("--log", help="also append events to this file")
    args = parser.parse_args()
    global LOG
    if args.log:
        LOG = open(args.log, "a", encoding="utf-8")
    host, _, port = args.target.rpartition(":")
    target = (host, int(port))
    link = Link(args)
    counter = [0]
    server = await asyncio.start_server(
        lambda r, w: serve(r, w, link, target, counter), args.bind, args.listen
    )
    plan = (
        f"cut at +{args.cut_after:.0f} s for {args.cut_for:.0f} s"
        if args.cut_after is not None
        else "no timed cut"
    )
    say(f"relaying {args.bind}:{args.listen} -> {args.target}; {plan}")
    asyncio.ensure_future(link.watch())
    async with server:
        await server.serve_forever()


if __name__ == "__main__":
    try:
        asyncio.run(main())
    except KeyboardInterrupt:
        sys.exit(0)
