#!/usr/bin/env python3
"""Fails if a page in docs/ links to a missing page, file or #anchor."""
import re, sys
from html.parser import HTMLParser
from pathlib import Path
from urllib.parse import urldefrag, urlparse

DOCS = Path(__file__).resolve().parents[2] / "docs"

class Page(HTMLParser):
    def __init__(self):
        super().__init__()
        self.ids, self.links = set(), []
    def handle_starttag(self, tag, attrs):
        a = dict(attrs)
        if "id" in a: self.ids.add(a["id"])
        for k in ("href", "src"):
            if k in a: self.links.append(a[k])

pages = {}
for f in sorted(DOCS.glob("*.html")):
    p = Page(); p.feed(f.read_text()); pages[f.name] = p

errors = []
for name, page in pages.items():
    for link in page.links:
        u = urlparse(link)
        if u.scheme or link.startswith(("mailto:", "//")): continue
        path, frag = urldefrag(link)
        target = "index.html" if path in ("", "./") and name else path
        if path == "": target = name
        if target in pages:
            if frag and frag not in pages[target].ids:
                errors.append(f"{name}: no #{frag} in {target}")
        elif not (DOCS / target).is_file():
            errors.append(f"{name}: missing {link}")
for e in errors: print(e)
print(f"checked {len(pages)} pages, {len(errors)} problem(s)")
sys.exit(1 if errors else 0)
