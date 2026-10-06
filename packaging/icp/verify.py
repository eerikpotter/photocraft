#!/usr/bin/env python3
"""Verify local canister delivery against the actual Trunk build, including large Wasm."""

import gzip
import hashlib
import http.client
import json
from pathlib import Path
import subprocess
from urllib.parse import urlparse


def main():
    root = Path(__file__).resolve().parents[2]
    site = root / "dist/web"
    network = json.loads(subprocess.check_output(["icp", "network", "status", "local", "--json"], cwd=root))
    if not network.get("managed"):
        raise RuntimeError("This smoke check only targets the managed local network")
    canister = subprocess.check_output(["icp", "canister", "status", "frontend", "--id-only", "-e", "local"], cwd=root, text=True).strip()
    gateway = urlparse(network["gateway_url"])
    host = f"{canister}.localhost:{gateway.port}"

    def get(path):
        connection = http.client.HTTPConnection("127.0.0.1", gateway.port, timeout=120)
        connection.request("GET", path, headers={"Host": host, "Accept-Encoding": "gzip"})
        response = connection.getresponse()
        headers = {k.lower(): v for k, v in response.getheaders()}
        body = response.read()
        status = response.status
        connection.close()
        if headers.get("content-encoding") == "gzip":
            body = gzip.decompress(body)
        return status, headers, body

    assets = [site / "index.html", *site.rglob("*.js"), *site.rglob("*.wasm")]
    if len(assets) < 3:
        raise RuntimeError("Build the application first")
    for asset in assets:
        path = "/" if asset.name == "index.html" else "/" + asset.relative_to(site).as_posix()
        status, headers, body = get(path)
        if status != 200 or body != asset.read_bytes():
            raise RuntimeError(f"{path}: status {status}, body differs from local build")
        if not headers.get("ic-certificate"):
            raise RuntimeError(f"{path}: missing certified response header")
        if asset.suffix == ".wasm":
            if headers.get("content-type") != "application/wasm" or headers.get("content-encoding") != "gzip":
                raise RuntimeError("Wasm must be served with the proper MIME type and compression")
        if asset.suffix in (".wasm", ".js") and "immutable" not in headers.get("cache-control", ""):
            raise RuntimeError(f"{path}: missing immutable cache policy")
        if asset.name == "index.html":
            policy = headers.get("content-security-policy", "")
            if "'wasm-unsafe-eval'" not in policy or "'sha256-" not in policy or "no-cache" not in headers.get("cache-control", ""):
                raise RuntimeError("HTML lacks Wasm/bootstrap CSP or revalidation")
        print(f"PASS {path}: {len(body):,} bytes, certified, SHA-256 {hashlib.sha256(body).hexdigest()[:16]}")
    if get("/missing-photocraft-smoke-test.wasm")[0] != 404:
        raise RuntimeError("A missing asset must return 404, not the HTML application shell")
    print("PASS missing asset returns 404")
    print(f"Open http://{host}/")


if __name__ == "__main__":
    main()
