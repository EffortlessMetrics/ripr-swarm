"""Check the executable installed by pip before the native package smoke test."""

import hashlib
import json
import os
from pathlib import Path
import re
import sys
import sysconfig
from urllib.parse import urlsplit


def verify(root, scripts, target, version, commit, wheel_tag):
    identities = list((root / "qualified-payload").rglob("payload-identity.json"))
    if len(identities) != 1:
        raise ValueError(f"expected one qualified payload identity, found {len(identities)}")
    identity = json.loads(identities[0].read_bytes())
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
    if not urlsplit(download["url"]).path.endswith(f"-py3-none-{wheel_tag}.whl"):
        raise ValueError(f"pip chose {download['url']}, expected {wheel_tag} wheel")
    wheel_sha = download["archive_info"]["hashes"]["sha256"]
    if not re.fullmatch(r"[0-9a-f]{64}", wheel_sha):
        raise ValueError("pip report has no valid wheel SHA-256")
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
