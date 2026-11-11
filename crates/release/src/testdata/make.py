#!/usr/bin/env python3
"""Makes the signed-manifest fixtures for `updates` tests: a throwaway
minisign key pair (never used for anything else), a manifest for
v0.2.0-alpha.2, and signatures over it in minisign's format (Ed25519 over
the BLAKE2b-512 of the file, then over that signature and the trusted
comment). Needs Python's `cryptography`. Run it from anywhere; it rewrites
the files beside it.
"""

import base64
import hashlib
import json
from pathlib import Path

from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PrivateKey
from cryptography.hazmat.primitives.serialization import Encoding, PublicFormat

HERE = Path(__file__).resolve().parent
# Fixed seeds so the files don't change from run to run.
SEED = bytes(range(32))
OTHER_SEED = bytes(range(1, 33))
KEY_ID = bytes.fromhex("0123456789abcdef")


def keypair(seed):
    private = Ed25519PrivateKey.from_private_bytes(seed)
    public = private.public_key().public_bytes(Encoding.Raw, PublicFormat.Raw)
    return private, public


def public_file(public, key_id):
    b64 = base64.b64encode(b"Ed" + key_id + public).decode()
    return f"untrusted comment: minisign public key {key_id[::-1].hex().upper()}\n{b64}\n"


def sign(private, key_id, data, trusted):
    digest = hashlib.blake2b(data, digest_size=64).digest()
    signature = private.sign(digest)
    global_signature = private.sign(signature + trusted.encode())
    return (
        "untrusted comment: signature from minisign secret key\n"
        + base64.b64encode(b"ED" + key_id + signature).decode()
        + f"\ntrusted comment: {trusted}\n"
        + base64.b64encode(global_signature).decode()
        + "\n"
    )


def artifact(name, size):
    return {"file": name, "sha256": "00" * 32, "size": size}


manifest = {
    "schema": 1,
    "product": "axcenity-engine",
    "version": "0.2.0-alpha.2",
    "channel": "development",
    "tag": "v0.2.0-alpha.2",
    "commit": "0123456789abcdef0123456789abcdef01234567",
    "published": "2026-12-01T12:00:00Z",
    "title": "Faster startup",
    "summary": "Projects open twice as fast.",
    "notes": {"performance": [{"subsystem": "editor", "title": "Projects open twice as fast"}]},
    "known_issues": ["AX-0001"],
    "formats": {"project": 5},
    "compatibility": {"min_update_protocol": 1},
    "artifacts": {
        "windows-x86_64": {
            "update": artifact("AxcenityEditor-0.2.0-alpha.2-windows-x86_64-update.zip", 100)
        }
    },
    "history": artifact("axcenity-history.json", 10),
    "provenance": {"reproducible": "not verified"},
    "added_by_a_later_schema": True,
}
data = (json.dumps(manifest, indent=2) + "\n").encode()

private, public = keypair(SEED)
other_private, other_public = keypair(OTHER_SEED)
(HERE / "manifest.json").write_bytes(data)
(HERE / "release-key.pub").write_text(public_file(public, KEY_ID))
(HERE / "manifest.json.minisig").write_text(sign(private, KEY_ID, data, "axcenity-engine v0.2.0-alpha.2"))
(HERE / "wrong-comment.minisig").write_text(sign(private, KEY_ID, data, "axcenity-engine v0.2.0-alpha.1"))
(HERE / "other-key.minisig").write_text(
    sign(other_private, KEY_ID, data, "axcenity-engine v0.2.0-alpha.2")
)
print("wrote fixtures in", HERE)
