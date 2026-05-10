#!/usr/bin/env python3
"""Git credential helper backed by Jamboree's GitHub App pass entries."""

from __future__ import annotations

import base64
import datetime as dt
import json
import os
import subprocess
import sys
import tempfile
import time
from pathlib import Path


APP_ID_KEY = "jam/pickers/github-app-id"
INSTALLATION_ID_KEY = "jam/pickers/github-app-installation-id"
PRIVATE_KEY_KEY = "jam/pickers/github-app-key"
TOKEN_CACHE = Path.home() / ".cache" / "jam" / "github-app-token.json"


def b64url(raw: bytes) -> str:
    return base64.urlsafe_b64encode(raw).rstrip(b"=").decode("ascii")


def pass_show(key: str) -> str:
    result = subprocess.run(
        ["pass", "show", key],
        check=True,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    return result.stdout.strip()


def read_cache() -> str | None:
    try:
        data = json.loads(TOKEN_CACHE.read_text())
    except (OSError, json.JSONDecodeError):
        return None
    token = data.get("token")
    expires_at = data.get("expires_at_unix")
    if isinstance(token, str) and isinstance(expires_at, (int, float)):
        if expires_at - time.time() > 120:
            return token
    return None


def write_cache(token: str, expires_at: str) -> None:
    try:
        parsed = dt.datetime.fromisoformat(expires_at.replace("Z", "+00:00"))
    except ValueError:
        return
    TOKEN_CACHE.parent.mkdir(parents=True, exist_ok=True)
    TOKEN_CACHE.write_text(
        json.dumps(
            {"token": token, "expires_at": expires_at, "expires_at_unix": parsed.timestamp()},
            separators=(",", ":"),
        )
    )
    TOKEN_CACHE.chmod(0o600)


def signed_jwt(app_id: str, private_key_pem: str) -> str:
    now = int(time.time())
    header = {"alg": "RS256", "typ": "JWT"}
    payload = {"iat": now - 60, "exp": now + 540, "iss": app_id}
    signing_input = (
        f"{b64url(json.dumps(header, separators=(',', ':')).encode())}."
        f"{b64url(json.dumps(payload, separators=(',', ':')).encode())}"
    ).encode("ascii")

    with tempfile.NamedTemporaryFile("w", delete=False) as key_file:
        key_path = Path(key_file.name)
        key_file.write(private_key_pem.rstrip() + "\n")
    key_path.chmod(0o600)
    try:
        signature = subprocess.run(
            ["openssl", "dgst", "-sha256", "-sign", str(key_path)],
            input=signing_input,
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        ).stdout
    finally:
        key_path.unlink(missing_ok=True)

    return f"{signing_input.decode('ascii')}.{b64url(signature)}"


def mint_installation_token() -> str:
    cached = read_cache()
    if cached:
        return cached

    app_id = pass_show(APP_ID_KEY)
    installation_id = pass_show(INSTALLATION_ID_KEY)
    private_key = pass_show(PRIVATE_KEY_KEY)
    jwt = signed_jwt(app_id, private_key)
    url = f"https://api.github.com/app/installations/{installation_id}/access_tokens"
    response = subprocess.run(
        [
            "curl",
            "-fsSL",
            "-X",
            "POST",
            "-H",
            f"Authorization: Bearer {jwt}",
            "-H",
            "Accept: application/vnd.github+json",
            "-H",
            "X-GitHub-Api-Version: 2022-11-28",
            url,
        ],
        check=True,
        text=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    ).stdout
    data = json.loads(response)
    token = data["token"]
    expires_at = data.get("expires_at")
    if isinstance(expires_at, str):
        write_cache(token, expires_at)
    return token


def read_credential_request() -> dict[str, str]:
    request: dict[str, str] = {}
    for line in sys.stdin:
        line = line.rstrip("\n")
        if not line:
            break
        key, _, value = line.partition("=")
        request[key] = value
    return request


def main() -> int:
    operation = sys.argv[1] if len(sys.argv) > 1 else "get"
    request = read_credential_request()
    if operation != "get":
        return 0
    if request.get("protocol") != "https" or request.get("host") != "github.com":
        return 0

    try:
        token = mint_installation_token()
    except Exception as exc:  # noqa: BLE001 - credential helpers report compactly to stderr.
        print(f"jam github app credential helper failed: {exc}", file=sys.stderr)
        return 1

    print("username=x-access-token")
    print(f"password={token}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
