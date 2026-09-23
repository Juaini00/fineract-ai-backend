#!/usr/bin/env bash
#
# Integration test lewat Bruno CLI.
#
# Tahap utama ditambah dua tahap retrieval dengan alasan yang mengikat:
#
#   1. `health`, `auth`, `chat` dijalankan dengan WORKER_ENABLED=false. Folder
#      `chat` menguji semantik PENERIMAAN — job tetap `Queued`, satu job
#      nonterminal per session, cancel memindahkan ke `Cancelling`. Dengan
#      worker menyala, job diselesaikan dalam hitungan milidetik dan hasil test
#      bergantung pada balapan, bukan pada perilaku yang diuji.
#   2. `engine`, `clarification`, `resolver` dan `sse` dijalankan dengan worker
#      menyala dan jeda antar-request, untuk membuktikan job benar-benar
#      bergerak sampai terminal tanpa campur tangan klien.
#   3. `sse` DIULANG dengan SSE_NOTIFICATIONS_ENABLED=false. Assertion-nya
#      identik dengan tahap 2, dan itulah buktinya: notifikasi bukan sumber
#      kebenaran, jadi menghilangkannya tidak boleh mengubah satu pun hasil —
#      hanya latensinya. Tanpa tahap ini, "fallback polling" hanya klaim.
#   4. `retrieval-unavailable` menjalankan worker dengan embedding dimatikan;
#      `retrieval-vector` dan `retrieval-healthy` hanya berjalan bila API key
#      tersedia dan versi katalog sudah memiliki embedding lengkap.
#
# Pakai:
#   scripts/integration-test.sh                 # seluruh tahap
#   scripts/integration-test.sh auth            # satu folder (tahap intake)
#   scripts/integration-test.sh engine          # tahap engine saja
#   PORT=3210 scripts/integration-test.sh       # port tertentu
#   KEEP_RUNNING=1 scripts/integration-test.sh  # biarkan app terakhir hidup
#
# Prasyarat: PostgreSQL aplikasi hidup dan sudah dimigrasi (`sqlx migrate run`).

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
COLLECTION="$ROOT/fineract-assistant-api"
PORT="${PORT:-3107}"
BASE_URL="http://127.0.0.1:$PORT"
LOG="${LOG:-/tmp/jarvis-integration.log}"

# Muat `.env` ke runner agar gate retrieval-healthy melihat konfigurasi yang
# sama dengan aplikasi, bukan hanya environment proses pemanggil.
if [ -f "$ROOT/.env" ]; then
    set -a
    # shellcheck disable=SC1091
    . "$ROOT/.env"
    set +a
fi

if [ "$#" -gt 0 ]; then
    INTAKE_FOLDERS=()
    ENGINE_FOLDERS=()
    RETRIEVAL_UNAVAILABLE_FOLDERS=()
    RETRIEVAL_HEALTHY_FOLDERS=()
    for folder in "$@"; do
        if [ "$folder" = "retrieval-unavailable" ]; then
            RETRIEVAL_UNAVAILABLE_FOLDERS+=("$folder")
        elif [ "$folder" = "retrieval-vector" ] || [ "$folder" = "retrieval-healthy" ]; then
            RETRIEVAL_HEALTHY_FOLDERS+=("$folder")
        elif [ "$folder" = "engine" ] || [ "$folder" = "clarification" ] || [ "$folder" = "resolver" ] || [ "$folder" = "sse" ]; then
            ENGINE_FOLDERS+=("$folder")
        else
            INTAKE_FOLDERS+=("$folder")
        fi
    done
else
    INTAKE_FOLDERS=(health auth chat)
    ENGINE_FOLDERS=(engine clarification resolver sse)
    RETRIEVAL_UNAVAILABLE_FOLDERS=(retrieval-unavailable)
    RETRIEVAL_HEALTHY_FOLDERS=(retrieval-vector retrieval-healthy)
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

# Katalog diperiksa lebih dulu: capability yang tidak lolos tidak layak
# dieksekusi, dan menemukannya setelah puluhan request HTTP hanya menunda kabar
# buruk.
echo "==> memeriksa katalog"
"$ROOT/target/debug/app" catalog

APP_PID=""

stop_app() {
    [ -n "$APP_PID" ] || return 0
    # SIGTERM, bukan SIGKILL: jalur graceful shutdown ikut terlatih setiap run.
    kill -TERM "$APP_PID" 2>/dev/null || true
    wait "$APP_PID" 2>/dev/null || true
    APP_PID=""
}

cleanup() {
    if [ -n "${KEEP_RUNNING:-}" ]; then
        echo "==> app dibiarkan hidup (pid $APP_PID) karena KEEP_RUNNING diset"
        return
    fi
    stop_app
}
trap cleanup EXIT

start_app() {  # $1 = nilai WORKER_ENABLED, sisanya = VAR=nilai tambahan
    local worker="$1"
    shift
    echo "==> menyalakan app pada $BASE_URL (worker=$worker $*, log: $LOG)"
    env APP_PORT="$PORT" WORKER_ENABLED="$worker" "$@" "$ROOT/target/debug/app" >>"$LOG" 2>&1 &
    APP_PID=$!

    # Tunggu sampai SEHAT, bukan sekadar sampai port terbuka: port yang sudah
    # menerima koneksi sementara PostgreSQL belum terjangkau menghasilkan
    # kegagalan test yang menyesatkan.
    for attempt in $(seq 1 40); do
        if [ "$(curl -s -o /dev/null -w '%{http_code}' "$BASE_URL/health" 2>/dev/null)" = "200" ]; then
            return 0
        fi
        if ! kill -0 "$APP_PID" 2>/dev/null; then
            echo "app berhenti saat startup:" >&2
            tail -20 "$LOG" >&2
            exit 1
        fi
        sleep 0.5
    done

    echo "/health tidak pernah 200 dalam 20 detik:" >&2
    tail -20 "$LOG" >&2
    exit 1
}

# --disable-cookies: rotasi dan reuse-detection menuntut kontrol penuh atas
# refresh token yang dikirim; cookie jar otomatis akan menimpa token lama dan
# membuat uji reuse tidak pernah benar-benar berjalan.
run_folders() {  # $1 = jeda ms, sisanya = folder
    local delay="$1"
    shift
    # Subshell: `bru` harus dijalankan dari direktori koleksi, tetapi app dibaca
    # dengan path katalog relatif terhadap root repo — cwd tidak boleh bocor ke
    # tahap berikutnya.
    (
        cd "$COLLECTION"
        bru run "$@" -r \
            --env local \
            --env-var "baseUrl=$BASE_URL" \
            --disable-cookies \
            --delay "$delay" \
            --bail
    )
}

: >"$LOG"

if [ "${#INTAKE_FOLDERS[@]}" -gt 0 ]; then
    start_app false
    echo "==> bru run ${INTAKE_FOLDERS[*]} (tanpa worker)"
    run_folders 0 "${INTAKE_FOLDERS[@]}"
    stop_app
fi

if [ "${#ENGINE_FOLDERS[@]}" -gt 0 ]; then
    start_app true
    echo "==> bru run ${ENGINE_FOLDERS[*]} (worker menyala)"
    # Jeda memberi worker kesempatan mengklaim dan menyelesaikan job sebelum
    # request berikutnya membacanya.
    run_folders 1500 "${ENGINE_FOLDERS[@]}"

    # Tahap 3 — SSE tanpa notifikasi sama sekali. Assertion-nya sama; yang
    # berubah hanya dari mana stream tahu ada event baru.
    for folder in "${ENGINE_FOLDERS[@]}"; do
        if [ "$folder" = "sse" ]; then
            stop_app
            start_app true SSE_NOTIFICATIONS_ENABLED=false
            echo "==> bru run sse (tanpa notifikasi; hanya fallback polling)"
            run_folders 1500 sse
        fi
    done
fi

if [ "${#RETRIEVAL_UNAVAILABLE_FOLDERS[@]}" -gt 0 ]; then
    stop_app
    start_app true EMBEDDING_API_KEY=
    echo "==> bru run retrieval-unavailable (embedding sengaja dinonaktifkan)"
    run_folders 1500 "${RETRIEVAL_UNAVAILABLE_FOLDERS[@]}"
fi

if [ "${#RETRIEVAL_HEALTHY_FOLDERS[@]}" -gt 0 ]; then
    stop_app
    if [ -n "${EMBEDDING_API_KEY:-}" ]; then
        start_app true
        echo "==> bru run retrieval-healthy (embedding terkonfigurasi dan terindeks)"
        run_folders 1500 "${RETRIEVAL_HEALTHY_FOLDERS[@]}"
    else
        echo "==> retrieval-healthy dilewati: EMBEDDING_API_KEY kosong; bukti out_of_scope tetap pending"
    fi
fi
