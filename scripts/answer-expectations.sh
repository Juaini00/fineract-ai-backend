#!/usr/bin/env bash
#
# FIN-52 (L4.0) — hitung ulang jawaban yang BENAR untuk setiap capability,
# langsung di Fineract, sesaat sebelum tahap Bruno `answers` berjalan.
#
# Setiap `tests/answers/<capability_id>.sql` adalah SQL langsung yang ditulis
# TERPISAH dari `queries/**` (metode knowledge/VERIFICATION.md: dua angka
# berdampingan, dua penulisan). Baris pertama berkas WAJIB berbentuk
#
#   -- params: {"from_date": ":month_start", "to_date": ":today", "limit": 100}
#
# yaitu parameter non-office yang diikat JOB untuk capability itu, persis seperti
# muncul di `evidence_json.lineage[0].parameters`. Bruno membandingkan keduanya,
# jadi SQL pembanding tidak bisa diam-diam memakai parameter yang berbeda.
# Token `:today` / `:month_start` diganti tanggal hari ini (UTC — sama dengan
# `business_today` planner) dan awal bulannya; `:today-12m` / `:today-30d` /
# `:today-1y` / `:today-2w` untuk default relatif (`relative_date` planner,
# yang juga menerima satuan minggu `w`).
# Di dalam SQL tersedia variabel psql:
#   :'today'  :'month_start'  :'office_ids'   (office_ids = '{1,2,...}', seluruh
#   office — scope admin lokal; pakai `= ANY(:'office_ids'::bigint[])`)
#
# SQL berupa SATU SELECT tanpa titik koma penutup (boleh ada `;` di tengah
# baris, mis. dalam literal string — hanya `;` pada AKHIR baris TERAKHIR
# berkas yang dibuang); urutan barisnya harus sama dengan urutan jawaban
# capability (ORDER BY yang sama maknanya). Urutan baris dijamin lewat
# `row_number()` di sekeliling body, bukan mengandalkan PostgreSQL
# mempertahankan urutan subquery ke `json_agg` (tidak dijamin oleh spec).
#
# Populasi row-cap (FIN-133): capability yang dipotong hard_cap/guards.
# max_limit (mis. savings_client_activity, savings_activity_list) butuh
# ukuran populasi PENUH (tanpa limit) untuk lib/answers.js::expectRowCap.
# Berkas tambahan `tests/answers/<capability_id>__population.sql` — header
# `-- params: {}` (tidak dibandingkan ke parameter job manapun, capability
# key-nya `<capability_id>__population` tidak pernah dipakai check()/
# checkSubset()) — mengembalikan SATU baris `{"n": <jumlah baris populasi>}`.
# Diproses lewat mesin yang sama seperti oracle biasa, tidak ada percabangan
# khusus di skrip ini.
#
# Keluaran: fineract-assistant-api/.answers/expected.js (gitignored), dibaca
# `lib/answers.js` lewat `require` — Bruno tidak dapat membaca berkas lain.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SQL_DIR="$ROOT/tests/answers"
OUT_DIR="$ROOT/fineract-assistant-api/.answers"
FINERACT_URL="${ANSWERS_FINERACT_URL:-postgres://root:password@127.0.0.1:5432/fineract_default}"
# sqlx (job) membuka sesi ber-TimeZone UTC; psql lokal ikut server TimeZone
# (bisa Asia/Makassar dll). Tanpa ini, oracle yang meng-cast timestamptz ke
# date (created_on_utc, transaction_date_time) bisa berbeda tanggal dari job.
export PGTZ=UTC

command -v psql >/dev/null || { echo "psql tidak ditemukan" >&2; exit 127; }
command -v jq >/dev/null || { echo "jq tidak ditemukan" >&2; exit 127; }

today="$(date -u +%F)"
month_start="$(date -u +%Y-%m-01)"
office_ids="$(psql -X -A -t -v ON_ERROR_STOP=1 -d "$FINERACT_URL" \
    -c "SELECT '{' || string_agg(id::text, ',' ORDER BY id) || '}' FROM m_office")"

mkdir -p "$OUT_DIR"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/rows" "$tmp/params"
count=0

for file in "$SQL_DIR"/*.sql; do
    [ -e "$file" ] || continue
    capability="$(basename "$file" .sql)"

    header="$(head -1 "$file")"
    case "$header" in
        "-- params: "*) ;;
        *) echo "$file: baris pertama wajib '-- params: {...}'" >&2; exit 1 ;;
    esac
    declared="$(printf '%s' "${header#-- params: }" \
        | sed -e "s/\":today\"/\"$today\"/g" -e "s/\":month_start\"/\"$month_start\"/g")"
    # Token relatif `":today-12m"` / `-30d` / `-1y` — padanan `relative_date`
    # planner (subtract_months menjaga hari dan dijepit ke akhir bulan, sama
    # dengan `date - interval` PostgreSQL). Dihitung setiap run, tidak pernah
    # ditulis sebagai tanggal literal yang basi besok.
    for token in $(printf '%s' "$declared" | grep -oE '":today-[0-9]+[dmwy]"' | sort -u); do
        spec="${token#\":today-}"
        spec="${spec%\"}"
        amount="${spec%?}"
        case "${spec: -1}" in d) unit=days ;; w) unit=weeks ;; m) unit=months ;; y) unit=years ;; esac
        value="$(psql -X -A -t -v ON_ERROR_STOP=1 -d "$FINERACT_URL" \
            -c "SELECT ('$today'::date - interval '$amount $unit')::date")"
        declared="${declared//$token/\"$value\"}"
    done
    printf '%s' "$declared" | jq -e 'type == "object"' >/dev/null \
        || { echo "$file: params bukan objek JSON" >&2; exit 1; }
    # Bungkus menjadi satu dokumen JSON; hanya `;` di AKHIR BARIS TERAKHIR
    # dibuang (mid-body `...;` di tengah literal string tetap utuh).
    body="$(tail -n +2 "$file" | sed -e '$ s/[[:space:]]*;[[:space:]]*$//')"
    # Urutan baris dijamin lewat row_number() di sekeliling body, bukan
    # mengandalkan PostgreSQL mempertahankan urutan subquery ke json_agg
    # (subquery order preservation bukan jaminan spec, cuma kebiasaan plan).
    {
        printf 'SELECT coalesce(json_agg(__fin52_row ORDER BY __fin52_ord), %s) FROM (\n' "'[]'::json"
        printf '    SELECT row_to_json(__fin52_body) AS __fin52_row, row_number() OVER () AS __fin52_ord\n'
        printf '    FROM (\n'
        printf '%s\n' "$body"
        printf '    ) __fin52_body\n'
        printf ') __fin52_wrap;\n'
    } >"$tmp/q.sql"

    result="$(psql -X -A -t -q -v ON_ERROR_STOP=1 -d "$FINERACT_URL" \
        -v today="$today" -v month_start="$month_start" -v office_ids="$office_ids" \
        -f "$tmp/q.sql")" || { echo "$file: SQL gagal" >&2; exit 1; }

    jq -c --arg k "$capability" '{($k): .}' <<<"$result" >"$tmp/rows/$capability.json"
    jq -c --arg k "$capability" '{($k): .}' <<<"$declared" >"$tmp/params/$capability.json"
    count=$((count + 1))
done

[ "$count" -gt 0 ] || { echo "tidak ada tests/answers/*.sql" >&2; exit 1; }

jq -s 'add' "$tmp"/rows/*.json >"$tmp/rows.json"
jq -s 'add' "$tmp"/params/*.json >"$tmp/params.json"

jq -n --arg today "$today" --arg month_start "$month_start" --arg office_ids "$office_ids" \
    --slurpfile rows "$tmp/rows.json" --slurpfile params "$tmp/params.json" \
    '{meta: {today: $today, month_start: $month_start, office_ids: $office_ids}, params: $params[0], rows: $rows[0]}' \
    | { printf 'module.exports = '; cat; printf ';\n'; } >"$tmp/expected.js"
# Ganti secara atomik: run Bruno yang sedang membaca berkas lama tidak pernah
# melihat berkas setengah jadi.
mv "$tmp/expected.js" "$OUT_DIR/expected.js"

echo "==> $count jawaban pembanding dihitung dari SQL langsung (today=$today, month_start=$month_start)"
