#!/usr/bin/env python3
"""Prepare Trunk output for ICP without changing the generated application."""

import base64
import hashlib
import json
from html.parser import HTMLParser
from pathlib import Path
import shutil
import sys


class InlineScripts(HTMLParser):
    def __init__(self):
        super().__init__(convert_charrefs=False)
        self.current = None
        self.scripts = []

    def handle_starttag(self, tag, attrs):
        if tag == "script" and "src" not in dict(attrs):
            self.current = []

    def handle_data(self, data):
        if self.current is not None:
            self.current.append(data)

    def handle_endtag(self, tag):
        if tag == "script" and self.current is not None:
            self.scripts.append("".join(self.current))
            self.current = None


def prepare(site):
    root = Path(__file__).resolve().parents[2]
    parser = InlineScripts()
    parser.feed((site / "index.html").read_text())
    if not parser.scripts or not list(site.glob("*.wasm")):
        raise ValueError("Expected Trunk HTML with a bootstrap script and a Wasm asset")
    hashes = ["'sha256-" + base64.b64encode(hashlib.sha256(s.encode()).digest()).decode() + "'" for s in parser.scripts]
    policy = "; ".join([
        "default-src 'self'",
        "script-src 'self' 'wasm-unsafe-eval' " + " ".join(hashes),
        "style-src 'self' 'unsafe-inline'",
        "img-src 'self' data: blob:",
        "font-src 'self' data:",
        "connect-src 'self'",
        "object-src 'none'",
        "base-uri 'self'",
    ])
    headers = (root / "packaging/web/_headers").read_text()
    headers += "\n/*\n  Referrer-Policy: no-referrer\n  Content-Security-Policy: " + policy + "\n"
    metadata = site / ".well-known" / "ii-app-metadata"
    metadata.parent.mkdir(parents=True, exist_ok=True)
    metadata.write_text(json.dumps({
        "name": "PhotoCraft Sovereign Cloud",
        "description": "Local creative tools with owner-controlled cloud files on the Internet Computer.",
    }))
    headers += "\n/.well-known/ii-app-metadata\n  Content-Type: application/json\n  Access-Control-Allow-Origin: *\n"
    (site / "_headers").write_text(headers)
    for name in ["LICENSE-MIT", "LICENSE-APACHE", "NOTICE", "ATTRIBUTION.md"]:
        shutil.copy2(root / name, site / name)
    for source in (root / "assets").rglob("*"):
        if source.is_file() and (source.name.startswith(("LICENSE", "OFL")) or source.suffix == ".license"):
            destination = site / "licenses" / source.relative_to(root / "assets")
            destination.parent.mkdir(parents=True, exist_ok=True)
            shutil.copy2(source, destination)
    icp_notices = site / "licenses" / "icp"
    icp_notices.mkdir(parents=True, exist_ok=True)
    for name in ["LICENSE", "README.md"]:
        shutil.copy2(root / "apps" / "photocraft-web" / "assets" / "icp" / name, icp_notices / name)
    print(f"Prepared {site}: Wasm CSP, caching, MIME type and attribution notices")


if __name__ == "__main__":
    prepare(Path(sys.argv[1]).resolve())
