#!/usr/bin/env python3
"""Verify local/mainnet canister delivery against the actual Trunk build, including large Wasm."""

import argparse
import gzip
import hashlib
import http.client
import json
from pathlib import Path
import subprocess
from urllib.parse import unquote, urlparse

# Canonical IC root key, pinned by ic-agent (not fetched from the endpoint under test).
MAINNET_ROOT_KEY = bytes.fromhex(
    "308182301d060d2b0601040182dc7c0503010201060c2b0601040182dc7c05030201036100"
    "814c0e6ec71fab583b08bd81373c255c3c371b2e84863c98a4f1e08b74235d14fb5d9c"
    "0cd546d9685f913a0c0b2cc5341583bf4b4392e467db96d65b9bb4cb717112f8472e0d"
    "5a4d14505ffd7484b01291091c5f87b98883463f98091a0baaae"
)


def verify_environment(cookie, cloud, mainnet):
    cookie = cookie.split(";", 1)[0]
    if not cookie.startswith("ic_env="):
        raise RuntimeError("Missing cloud runtime environment cookie")
    env = dict(item.split("=", 1) for item in unquote(cookie.removeprefix("ic_env=")).split("&"))
    key = bytes.fromhex(env.get("ic_root_key", ""))
    if env.get("PUBLIC_CANISTER_ID:cloud") != cloud or len(key) != 133:
        raise RuntimeError("Cloud runtime environment has the wrong backend or trust key")
    if mainnet and key != MAINNET_ROOT_KEY:
        raise RuntimeError("Mainnet frontend must use the canonical IC root key")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--environment", "-e", choices=["local", "ic"], default="local")
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[2]
    site = root / "dist/web"
    mainnet = args.environment == "ic"
    mappings = root / ".icp" / ("data" if mainnet else "cache") / "mappings" / f"{args.environment}.ids.json"
    ids = json.loads(mappings.read_text())
    canister, cloud = ids["frontend"], ids["cloud"]
    if mainnet:
        host = f"{canister}.icp.net"
        origin = f"https://{host}"
    else:
        network = json.loads(subprocess.check_output(["icp", "network", "status", "local", "--json"], cwd=root))
        if not network.get("managed"):
            raise RuntimeError("The local check requires a managed network")
        gateway = urlparse(network["gateway_url"])
        host = f"{canister}.localhost:{gateway.port}"
        origin = f"http://{host}"

    def get(path):
        connection = http.client.HTTPSConnection(host, timeout=120) if mainnet else http.client.HTTPConnection("127.0.0.1", gateway.port, timeout=120)
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
            verify_environment(headers.get("set-cookie", ""), cloud, mainnet)
        print(f"PASS {path}: {len(body):,} bytes, certified, SHA-256 {hashlib.sha256(body).hexdigest()[:16]}")
    if get("/missing-photocraft-smoke-test.wasm")[0] != 404:
        raise RuntimeError("A missing asset must return 404, not the HTML application shell")
    print("PASS missing asset returns 404")
    status, headers, body = get("/.well-known/ii-app-metadata")
    if status != 200 or json.loads(body).get("name") != "PhotoCraft Sovereign Cloud" or headers.get("access-control-allow-origin") != "*":
        raise RuntimeError("Internet Identity app metadata is unavailable")
    print("PASS cloud runtime environment and Internet Identity metadata")
    print(f"Open {origin}/")


if __name__ == "__main__":
    main()
