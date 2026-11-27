"""The rules section of the landing page: write Magician's rule catalogue into index.html.

Run it whenever a rule's words, name, severity or fair-play standing change, before the page is published:

    python site/rules.py --magician target/release/magician      (magician.exe on Windows)
    python site/rules.py --check                                  exit 1 if the page is not what the catalogue says

It runs `magician rules --json` (the binary on PATH unless --magician names one) and replaces two things in the page,
nothing else: the JSON inside <script id="rules-data"> (one [code, name, severity, what it costs in game, fair play]
row per rule, which the page's script draws as the rules map) and the count in <summary id="rules-count">.
"""

import argparse
import json
import re
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
LETTER = {"error": "e", "warn": "w", "info": "i"}


def catalogue(magician):
    p = subprocess.run([magician, "rules", "--json"], capture_output=True, text=True, encoding="utf-8")
    if p.returncode != 0:
        sys.exit(f"magician rules --json: exit {p.returncode}\n{p.stderr}")
    doc = json.loads(p.stdout)
    if doc.get("schema") != "magician.rules/1":
        sys.exit(f"magician rules --json: schema {doc.get('schema')!r}, expected magician.rules/1")
    rules = doc["rules"]
    if any("fair_play" not in r for r in rules):
        sys.exit("magician rules --json gives no fair_play: the binary is older than this script")
    return [[r["code"], r["name"], LETTER[r["severity"]], r["in_game"], 1 if r["fair_play"] else 0] for r in rules]


def render(html, rows):
    text = json.dumps(rows, ensure_ascii=False, separators=(",", ":")).replace("</", "<\\/")
    html, n = re.subn(r'(<script id="rules-data" type="application/json">).*?(</script>)', lambda m: m.group(1) + text + m.group(2), html, count=1, flags=re.S)
    if n != 1:
        sys.exit('index.html: no <script id="rules-data"> to fill')
    html, n = re.subn(r'(<summary id="rules-count">).*?(</summary>)', lambda m: f"{m.group(1)}All {len(rows)} rules{m.group(2)}", html, count=1, flags=re.S)
    if n != 1:
        sys.exit('index.html: no <summary id="rules-count"> to fill')
    return html


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--magician", default="magician", help="the magician binary (default: on PATH)")
    ap.add_argument("--page", default=str(HERE / "index.html"))
    ap.add_argument("--check", action="store_true", help="change nothing; exit 1 if the page differs from the catalogue")
    a = ap.parse_args()
    page = Path(a.page)
    html = page.read_bytes().decode("utf-8")
    rows = catalogue(a.magician)
    new = render(html, rows)
    if a.check:
        if new != html:
            sys.exit(f"{page.name}: the rules section is not what `magician rules --json` says; run python site/rules.py")
        print(f"{page.name}: the rules section matches the catalogue ({len(rows)} rules)")
        return
    page.write_bytes(new.encode("utf-8"))
    print(f"{page.name}: {len(rows)} rules, {sum(r[4] for r in rows)} of them fair play" + ("" if new != html else " (unchanged)"))


if __name__ == "__main__":
    main()
