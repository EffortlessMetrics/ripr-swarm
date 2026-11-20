"""Check the executable installed by pip before the native package smoke test."""

import hashlib
import json
import os
from pathlib import Path
import re
import sys
import sysconfig
from urllib.parse import unquote, urlsplit


def verify(root, scripts, target, version, commit, wheel_tag):
    identities = list((root / "qualified-payload").rglob("payload-identity.json"))
    if len(identities) != 1:
        raise ValueError(f"expected one qualified payload identity, found {len(identities)}")
    identity_bytes = identities[0].read_bytes()
    identity = json.loads(identity_bytes)
    executable = "ripr.exe" if target == "x86_64-pc-windows-msvc" else "ripr"
    if (identity["kind"], identity["schema_version"], identity["source"]["commit_sha"],
        identity["target"], identity["version"], identity["executable"]) != (
        "ripr_final_native_payload", 1, commit, target, version, executable
    ):
        raise ValueError(f"qualified payload identity does not match requested {target} {version}")
    rows = [row for row in identity["files"] if row["role"] == "executable"]
    if len(rows) != 1 or rows[0]["path"] != executable:
        raise ValueError(f"expected one qualified {executable} for {target}")
    installed = scripts / executable
    if installed.is_symlink() or not installed.is_file():
        raise ValueError(f"missing regular pip-installed executable at {installed}")
    with installed.open("rb") as binary:
        actual = hashlib.file_digest(binary, "sha256").hexdigest()
    if actual != rows[0]["sha256"]:
        raise ValueError(f"{target} pip-installed {executable} SHA-256 {actual}, expected {rows[0]['sha256']}")

    package, = json.loads((root / "pip-report.json").read_bytes())["install"]
    download = package["download_info"]
    producer_path = root / "packages" / "wheels" / f"wheel-native-receipt-{target}.json"
    producer = json.loads(producer_path.read_bytes())
    if (producer["kind"], producer["schema_version"], producer["target"], producer["version"],
        producer["source_commit"], producer["source_tree"], producer["platform_tag"],
        producer["payload_identity_sha256"]) != (
        "ripr_native_python_wheel", 1, target, version, commit, identity["source"]["tree_sha"],
        wheel_tag, hashlib.sha256(identity_bytes).hexdigest(),
    ):
        raise ValueError("wheel producer receipt does not match the qualified payload")
    metadata = package["metadata"]
    python_version = producer["python_version"]
    if re.sub(r"[-_.]+", "-", metadata["name"]).lower() != "ripr-rs" or metadata["version"] != python_version:
        raise ValueError("pip distribution name or version differs from the qualified wheel")
    selected = urlsplit(download["url"])
    wheel_name = f"ripr_rs-{python_version}-py3-none-{wheel_tag}.whl"
    if (selected.scheme != "file" or selected.netloc not in ("", "localhost")
        or Path(unquote(selected.path)).name != wheel_name or producer["wheel"] != wheel_name):
        raise ValueError(f"pip chose {download['url']}, expected {wheel_tag} wheel {wheel_name}")
    wheel_sha = download["archive_info"]["hashes"]["sha256"]
    if not re.fullmatch(r"[0-9a-f]{64}", wheel_sha) or wheel_sha != producer["wheel_sha256"]:
        raise ValueError("pip wheel SHA-256 differs from the producer receipt")
    receipt = {
        "kind": "ripr_installed_wheel_identity",
        "schema_version": 1,
        "source": identity["source"],
        "target": target,
        "version": version,
        "executable": executable,
        "executable_sha256": actual,
        "wheel_sha256": wheel_sha,
    }
    (root / "installed-wheel-identity.json").write_text(json.dumps(receipt, indent=2) + "\n", encoding="utf-8")
    return installed


if __name__ == "__main__":
    try:
        installed = verify(Path(os.environ["RUNNER_TEMP"]), Path(sysconfig.get_path("scripts")),
                           os.environ["TARGET"], os.environ["VERSION"].removeprefix("v"),
                           os.environ["CANDIDATE_SHA"], os.environ["WHEEL_TAG"])
    except (OSError, KeyError, TypeError, ValueError) as error:
        sys.exit(f"installed wheel qualification failed: {error}")
    # Bash reads this path on native Windows as well as macOS and Linux.
    sys.stdout.buffer.write((installed.as_posix() + "\n").encode("utf-8"))
