#!/usr/bin/env bash
#
# Pemetaan skenario acceptance → test.
#
# Aturan 1 di docs/build-order.md §1: "selesai" berarti skenario acceptance
# lulus, bukan test hijau. Skrip ini menegakkan bagian yang dapat ditegakkan
# mesin:
#
#   1. Mengumpulkan seluruh ID skenario yang tertulis di docs/.
#   2. Mengumpulkan ID yang disebut test — request Bruno (.yml) dan test Rust.
#   3. Melaporkan cakupan: skenario mana yang punya test, mana yang belum.
#   4. GAGAL bila docs/build-order.md §5 menandai sebuah lapisan ✅ padahal
#      masih ada skenario milik lapisan itu yang tidak disebut satu test pun.
#
# Yang TIDAK diperiksa: apakah test itu benar-benar membuktikan skenarionya.
# Tidak ada mesin yang bisa. Menyebut ID pada test yang tidak membuktikannya
# adalah kebohongan yang sama dengan menandai ✅ tanpa test — hanya lebih mahal.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

python3 - <<'PY'
import os, re, sys

ID_RE = re.compile(r'\b(CLR|RESP|AC|API|SSE|DS|OVR|MEM)-\d+(?:\.\d+)?\b')

# Prefix → lapisan pemilik, menurut docs/build-order.md §3.
OWNER = {
    'API':  ['L0'],
    'SSE':  ['L0'],
    'DS':   ['L3'],
    'OVR':  ['L4'],
    'RESP': ['L5', 'L6'],
    'CLR':  ['L7'],
    'MEM':  ['L7'],
    'AC':   ['L8'],
}

def walk(root, suffixes):
    for base, _, files in os.walk(root):
        if '/target' in base or '/node_modules' in base:
            continue
        for f in files:
            if f.endswith(suffixes):
                yield os.path.join(base, f)

# 1. ID skenario: baris docs yang mendeklarasikannya (bullet atau item bernomor).
declared = {}          # id -> file
duplicates = []
decl_line = re.compile(r'^\s*(?:[-*]|\d+\.)\s+`([A-Z]+-\d+(?:\.\d+)?)`')
for path in walk('docs', ('.md',)):
    for line in open(path):
        m = decl_line.match(line)
        if not m:
            continue
        sid = m.group(1)
        if not ID_RE.fullmatch(sid):
            continue
        if sid in declared:
            duplicates.append((sid, declared[sid], path))
        else:
            declared[sid] = path

# 2. ID yang dirujuk test.
referenced = {}        # id -> [file, ...]
sources = list(walk('fineract-assistant-api', ('.yml', '.yaml')))
sources += [p for p in walk('crates', ('.rs',))]
sources += [p for p in walk('tests', ('.sql',))] if os.path.isdir('tests') else []
for path in sources:
    try:
        text = open(path, encoding='utf-8').read()
    except UnicodeDecodeError:
        continue
    for sid in {m.group(0) for m in ID_RE.finditer(text)}:
        referenced.setdefault(sid, []).append(path)

unknown = sorted(set(referenced) - set(declared))
covered = sorted(set(declared) & set(referenced))
uncovered = sorted(set(declared) - set(referenced))

def sort_key(sid):
    pre, num = sid.split('-', 1)
    return (pre, [int(p) for p in num.split('.')])

print(f"==> skenario terdeklarasi: {len(declared)}")
by_prefix = {}
for sid in declared:
    by_prefix.setdefault(sid.split('-')[0], []).append(sid)
for pre in sorted(by_prefix):
    ids = by_prefix[pre]
    have = [s for s in ids if s in referenced]
    print(f"    {pre:<5} {len(have):>2}/{len(ids):<2} bertest   ({', '.join(OWNER.get(pre, ['?']))})")
print(f"==> cakupan: {len(covered)}/{len(declared)} skenario punya test")

if uncovered:
    print("==> belum punya test:")
    for sid in sorted(uncovered, key=sort_key):
        print(f"    {sid}  ({declared[sid]})")

failures = 0
if duplicates:
    failures += 1
    print("GAGAL: ID skenario ganda:")
    for sid, a, b in duplicates:
        print(f"    {sid}: {a} dan {b}")
if unknown:
    failures += 1
    print("GAGAL: test menyebut ID yang tidak ada di docs:")
    for sid in unknown:
        print(f"    {sid}: {', '.join(referenced[sid])}")

# 3. Gerbang: lapisan ✅ tidak boleh menyisakan skenario tanpa test.
row = re.compile(r'^\|\s*(L\d)\b[^|]*\|\s*([⬜🔨🧪✅❌])\s*\|')
status = {}
section5 = False
for line in open('docs/build-order.md'):
    if line.startswith('## 5.'):
        section5 = True
        continue
    if section5 and line.startswith('## '):
        break
    if section5:
        m = row.match(line)
        if m:
            status[m.group(1)] = m.group(2)

if not status:
    failures += 1
    print("GAGAL: tabel status docs/build-order.md §5 tidak terbaca")

for layer, mark in sorted(status.items()):
    if mark != '✅':
        continue
    missing = [s for s in uncovered if layer in OWNER.get(s.split('-')[0], [])]
    if missing:
        failures += 1
        print(f"GAGAL: {layer} ditandai ✅ tetapi skenarionya tanpa test: {', '.join(sorted(missing, key=sort_key))}")

if failures:
    sys.exit(1)
print("OK")
PY
