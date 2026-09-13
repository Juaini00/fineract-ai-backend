#!/usr/bin/env bash
#
# Integration test lewat Bruno CLI.
#
# Membangun biner, menyalakannya pada port bebas, menunggu /health benar-benar
# sehat, menjalankan koleksi `fineract-assistant-api/`, lalu mematikan proses —
# berhasil atau gagal. Exit code adalah exit code Bruno.
#
# Pakai:
#   scripts/integration-test.sh                 # seluruh koleksi
#   scripts/integration-test.sh auth            # satu folder
#   PORT=3210 scripts/integration-test.sh       # port tertentu
#   KEEP_RUNNING=1 scripts/integration-test.sh  # biarkan app hidup untuk debug
#
# Prasyarat: PostgreSQL aplikasi hidup dan sudah dimigrasi (`sqlx migrate run`).
# Script ini TIDAK memigrasi database Anda — `.env` lokal yang menentukan, dan
# migrasi otomatis hanya sah di APP_ENV=local (lihat Config::may_migrate_on_startup).

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
COLLECTION="$ROOT/fineract-assistant-api"
PORT="${PORT:-3107}"
BASE_URL="http://127.0.0.1:$PORT"
LOG="${LOG:-/tmp/jarvis-integration.log}"
if [ "$#" -gt 0 ]; then
    FOLDERS=("$@")
else
    FOLDERS=(health auth chat)
fi

command -v bru >/dev/null || {
    echo "bru (Bruno CLI) tidak ditemukan. Pasang: npm install -g @usebruno/cli" >&2
    exit 127
}

if lsof -nP -iTCP:"$PORT" -sTCP:LISTEN >/dev/null 2>&1; then
    echo "Port $PORT sudah dipakai proses lain. Jalankan ulang dengan PORT=<lain>." >&2
    exit 1
fi

echo "==> build"
cargo build -p app --quiet

echo "==> menyalakan app pada $BASE_URL (log: $LOG)"
APP_PORT="$PORT" "$ROOT/target/debug/app" >"$LOG" 2>&1 &
APP_PID=$!

cleanup() {
    if [ -n "${KEEP_RUNNING:-}" ]; then
        echo "==> app dibiarkan hidup (pid $APP_PID) karena KEEP_RUNNING diset"
        return
    fi
    # SIGTERM, bukan SIGKILL: jalur graceful shutdown ikut terlatih setiap run.
    kill -TERM "$APP_PID" 2>/dev/null || true
    wait "$APP_PID" 2>/dev/null || true
}
trap cleanup EXIT

# Tunggu sampai SEHAT, bukan sekadar sampai port terbuka: port yang sudah
# menerima koneksi sementara PostgreSQL belum terjangkau menghasilkan kegagalan
# test yang menyesatkan.
echo "==> menunggu /health"
for attempt in $(seq 1 40); do
    if [ "$(curl -s -o /dev/null -w '%{http_code}' "$BASE_URL/health" 2>/dev/null)" = "200" ]; then
        break
    fi
    if ! kill -0 "$APP_PID" 2>/dev/null; then
        echo "app berhenti saat startup:" >&2
        tail -20 "$LOG" >&2
        exit 1
    fi
    if [ "$attempt" -eq 40 ]; then
        echo "/health tidak pernah 200 dalam 20 detik:" >&2
        curl -s "$BASE_URL/health" >&2 || true
        tail -20 "$LOG" >&2
        exit 1
    fi
    sleep 0.5
done

echo "==> bru run ${FOLDERS[*]}"
cd "$COLLECTION"
# --disable-cookies: rotasi dan reuse-detection menuntut kontrol penuh atas
# refresh token yang dikirim; cookie jar otomatis akan menimpa token lama dan
# membuat uji reuse tidak pernah benar-benar berjalan.
bru run "${FOLDERS[@]}" -r \
    --env local \
    --env-var "baseUrl=$BASE_URL" \
    --disable-cookies \
    --bail
