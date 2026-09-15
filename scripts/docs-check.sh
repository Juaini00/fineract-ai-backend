#!/usr/bin/env bash
#
# Pemeriksaan dokumentasi yang dapat dilakukan mesin.
#
# Dua hal yang paling cepat membusuk dan paling mudah luput saat review:
#
#   1. Link antar-dokumen yang menunjuk file tidak ada. Dokumen yang saling
#      merujuk adalah cara paket desain ini tetap terhubung; link mati membuat
#      pembaca menyimpulkan dokumennya memang tidak ada.
#   2. Endpoint yang terdaftar di kode tetapi tidak muncul di
#      `docs/contracts/api-reference.md`. Dokumen itu adalah satu-satunya yang
#      boleh dipercaya frontend, dan ia hanya berguna selama ia diturunkan dari
#      kode — bukan dari ingatan.
#
# Yang TIDAK diperiksa skrip ini: kebenaran prosa. Tidak ada yang bisa. Status
# implementasi di docs/build-order.md tetap tanggung jawab manusia yang mengubahnya.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

failures=0

echo "==> link relatif"
python3 - <<'PY' || exit 1
import os, re, sys

targets = []
for root, _, files in os.walk('docs'):
    targets += [os.path.join(root, f) for f in files if f.endswith('.md')]
targets += ['README.md', 'AGENTS.md']

broken = 0
for path in targets:
    base = os.path.dirname(path)
    for link in re.findall(r'\]\(([^)#]+?)(?:#[^)]*)?\)', open(path).read()):
        if link.startswith(('http://', 'https://', 'mailto:')):
            continue
        if not os.path.exists(os.path.normpath(os.path.join(base, link))):
            print(f"    MATI  {path} -> {link}")
            broken += 1

print(f"    {broken} link mati")
sys.exit(1 if broken else 0)
PY

echo "==> endpoint terdokumentasi"
python3 - <<'PY' || exit 1
import re, subprocess, sys

routes = subprocess.run(
    ['grep', '-rho', r'\.route("[^"]*"', 'crates'],
    capture_output=True, text=True, check=True,
).stdout

paths = sorted({line.split('"')[1] for line in routes.splitlines()})
reference = open('docs/contracts/api-reference.md').read()

# Path axum memakai `{param}`; dokumen menulisnya sama. Yang dicocokkan adalah
# path literalnya, bukan method: satu route dapat melayani dua method.
missing = [path for path in paths if path not in reference]
for path in missing:
    print(f"    HILANG  {path}")

print(f"    {len(paths)} route di kode, {len(missing)} tidak ada di api-reference.md")
sys.exit(1 if missing else 0)
PY

echo "==> OK"
exit $failures
