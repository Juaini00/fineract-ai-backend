#!/usr/bin/env bash
#
# Integration test lewat Bruno CLI.
#
# Tahap-tahap dan alasannya yang mengikat:
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
#   4. `dataset-capped` menjalankan worker dengan seam lokal
#      LOCAL_DATASET_MAX_ROWS=1 (FIN-46): data lokal jauh di bawah cap
#      produksi, jadi cabang `truncated=true` DS-8.1 hanya terjangkau lewat
#      cap yang disempitkan. Tahap lain berjalan TANPA seam ini.
#   5. `answers` (FIN-52, gerbang L4) menjalankan setiap capability jawaban
#      lewat jalur job dan mengadu jawabannya dengan SQL langsung yang dihitung
#      ulang oleh scripts/answer-expectations.sh tepat sebelum tahap ini.
#   6. `redis-down` (FIN-57, OVR-6.5) menjalankan worker dengan REDIS_URL ke
#      port yang tidak mendengarkan: Redis diaktifkan tetapi tidak terjangkau.
#      Job wajib tetap selesai, event pulih dari PostgreSQL, dan klien yang
#      memutus stream tidak membatalkan job.
#   7. `crash-recovery` dan `crash-exhausted` (FIN-56, OVR-6.4) menjalankan
#      worker dengan seam lokal LOCAL_CRASH_AFTER_EXTERNAL_CALL: N panggilan
#      sumber pertama ditinggalkan sesudah query kembali dan sebelum T4 —
#      seperti worker yang mati di titik itu. Satu crash harus pulih lewat
#      attempt baru; crash sebanyak NODE_ATTEMPT_CAP harus berhenti di cap.
#      `crash-expired` dan `crash-cancelled` (FIN-141) memakai seam yang sama:
#      job yang ditutup reaper `Expired`/`Cancelled` saat attempt-nya `Running`
#      wajib menutup attempt itu `Abandoned`, bukan membiarkannya `Running`.
#      `worker-error` (FIN-141) memakai seam LOCAL_WORKER_ERROR_BEFORE_ADMISSION:
#      setiap klaim berakhir error sebelum admisi; heartbeat wajib berhenti dan
#      klaim ulang tidak boleh menggeser deadline TTL.
#
# Setelah tahap terakhir, log app seluruh tahap diperiksa untuk penanda
# `commit_isolation_violation` (OVR-6.7, I1 — crates/core/src/commit_isolation.rs):
# query Fineract, embedding, atau Redis saat transaksi commit terbuka. Satu
# kemunculan menggagalkan run, meskipun seluruh assertion Bruno hijau.
#
# Pakai:
#   scripts/integration-test.sh                 # seluruh tahap
#   scripts/integration-test.sh auth            # satu folder (tahap intake)
#   scripts/integration-test.sh engine          # tahap engine saja
#   scripts/integration-test.sh dataset-capped  # tahap cap dataset saja
#   scripts/integration-test.sh answers         # gerbang jawaban L4 saja
#   scripts/integration-test.sh answers/loans   # satu sub-folder, di tahap folder induknya
#   scripts/integration-test.sh engine/login.yml engine/policy-session.yml  # berkas tertentu (beserta prasyaratnya)
#   ANSWERS_ORACLE_PREFIX=loan scripts/integration-test.sh answers/loans  # pembanding hanya tests/answers/loan*.sql
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

# Muat `.env` ke runner agar tahap Bruno melihat konfigurasi yang sama dengan
# aplikasi, bukan hanya environment proses pemanggil.
if [ -f "$ROOT/.env" ]; then
    set -a
    # shellcheck disable=SC1091
    . "$ROOT/.env"
    set +a
fi

# Seam cap dataset hanya milik tahap `dataset-capped`. Bila `.env` menyetelnya,
# dotenvy di app membacanya kembali di SETIAP tahap dan tahap engine gagal
# dengan pesan yang tidak menunjuk penyebabnya — tolak sejak awal.
if [ -n "${LOCAL_DATASET_MAX_ROWS+x}" ]; then
    echo "LOCAL_DATASET_MAX_ROWS tidak boleh di-set di .env/environment; runner menyetelnya hanya untuk dataset-capped" >&2
    exit 1
fi

# Seam crash (FIN-56) sama: hanya milik tahap crash-*. Terbaca di tahap lain,
# ia diam-diam membuang hasil query pertama setiap app yang dinyalakan.
if [ -n "${LOCAL_CRASH_AFTER_EXTERNAL_CALL+x}" ] || [ -n "${LOCAL_WORKER_ERROR_BEFORE_ADMISSION+x}" ]; then
    echo "LOCAL_CRASH_AFTER_EXTERNAL_CALL/LOCAL_WORKER_ERROR_BEFORE_ADMISSION tidak boleh di-set di .env/environment; runner menyetelnya hanya untuk tahap crash-*/worker-error" >&2
    exit 1
fi

PLANNER_FOLDERS=()

if [ "$#" -gt 0 ]; then
    INTAKE_FOLDERS=()
    ENGINE_FOLDERS=()
    DATASET_CAPPED_FOLDERS=()
    ANSWERS_FOLDERS=()
    REDIS_DOWN_FOLDERS=()
    CRASH_FOLDERS=()
    for folder in "$@"; do
        # A sub-path (`answers/loans`, `engine/login.yml`) runs only that part
        # of a folder, in the stage of its top-level folder: a scoped run after
        # a narrow change. A bare folder name keeps its default behaviour.
        top="${folder%%/*}"
        if [ "$top" = "planner" ] || [[ "$top" == planner-* ]]; then
            PLANNER_FOLDERS+=("$folder")
            continue
        fi
        if [ "$top" = "answers" ]; then
            ANSWERS_FOLDERS+=("$folder")
        elif [ "$folder" = "redis-down" ]; then
            REDIS_DOWN_FOLDERS+=("$folder")
        elif [ "$folder" = "crash-recovery" ] || [ "$folder" = "crash-exhausted" ] \
            || [ "$folder" = "crash-expired" ] || [ "$folder" = "crash-cancelled" ] \
            || [ "$folder" = "worker-error" ]; then
            CRASH_FOLDERS+=("$folder")
        elif [ "$folder" = "dataset-capped" ]; then
            DATASET_CAPPED_FOLDERS+=("$folder")
        elif [ "$top" = "engine" ] || [ "$folder" = "clarification" ] || [ "$folder" = "resolver" ] || [ "$folder" = "sse" ] || [ "$folder" = "params" ]; then
            ENGINE_FOLDERS+=("$folder")
        else
            INTAKE_FOLDERS+=("$folder")
        fi
    done
else
    INTAKE_FOLDERS=(health auth chat)
    ENGINE_FOLDERS=(engine clarification resolver sse params)
    DATASET_CAPPED_FOLDERS=(dataset-capped)
    ANSWERS_FOLDERS=(answers)
    REDIS_DOWN_FOLDERS=(redis-down)
    CRASH_FOLDERS=(crash-recovery crash-exhausted crash-expired crash-cancelled worker-error)
    PLANNER_FOLDERS=(planner planner-unavailable planner-invalid planner-repair planner-no-fit planner-budget)
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
CATALOG_CHECK_OUTPUT="$("$ROOT/target/debug/app" catalog)"
echo "$CATALOG_CHECK_OUTPUT"

APP_PID=""
PROVIDER_PID=""

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
    if [ -n "$PROVIDER_PID" ]; then
        kill -TERM "$PROVIDER_PID" 2>/dev/null || true
        wait "$PROVIDER_PID" 2>/dev/null || true
    fi
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
    local extra=()
    if [ "${BRU_ENVIRONMENT:-local}" = "planner-manual" ]; then
        extra+=(--env-var "manualPlannerRequest=${MANUAL_PLANNER_REQUEST:-Total setoran tabungan bulan ini}")
    fi
    # Subshell: `bru` harus dijalankan dari direktori koleksi, tetapi app dibaca
    # dengan path katalog relatif terhadap root repo — cwd tidak boleh bocor ke
    # tahap berikutnya.
    (
        cd "$COLLECTION"
        bru run "$@" -r \
            --env "${BRU_ENVIRONMENT:-local}" \
            --env-var "baseUrl=$BASE_URL" \
            --env-var "plannerMode=${PLANNER_MODE:-real}" \
            --env-var "plannerExpectedFailure=${PLANNER_EXPECTED_FAILURE:-}" \
            --env-var "plannerExpectedAttempts=${PLANNER_EXPECTED_ATTEMPTS:-0}" \
            "${extra[@]}" \
            --disable-cookies \
            --delay "$delay" \
            ${BRU_SANDBOX:+--sandbox "$BRU_SANDBOX"} \
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
    # FIN-146 — engine/ dan resolver/ sekarang memuat request yang poll lewat
    # lib/poll.js (setTimeout); sandbox quickjs default tidak mengenal
    # setTimeout, sama seperti crash-*/answers/redis-down.
    BRU_SANDBOX=developer run_folders 1500 "${ENGINE_FOLDERS[@]}"

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

if [ "${#DATASET_CAPPED_FOLDERS[@]}" -gt 0 ]; then
    stop_app
    start_app true LOCAL_DATASET_MAX_ROWS=1
    echo "==> bru run dataset-capped (cap retensi dataset disempitkan ke 1 baris)"
    # FIN-146 — response.yml sekarang poll lewat lib/poll.js (setTimeout).
    BRU_SANDBOX=developer run_folders 1500 "${DATASET_CAPPED_FOLDERS[@]}"
fi

if [ "${#ANSWERS_FOLDERS[@]}" -gt 0 ]; then
    # Pembanding dihitung SEBELUM job dijalankan dan dengan tanggal yang sama
    # (UTC) dengan `business_today` planner; Bruno memeriksa bahwa parameter
    # yang benar-benar diikat job sama dengan yang diasumsikan SQL.
    "$ROOT/scripts/answer-expectations.sh"
    stop_app
    start_app true
    echo "==> bru run answers (jawaban job diadu dengan SQL langsung)"
    # Sandbox `developer` (node vm), bukan QuickJS bawaan: modul pembanding
    # yang di-`require` membuat runtime QuickJS bru 4.0.0 abort saat dibuang
    # (`list_empty(&rt->gc_obj_list)`) SESUDAH test-nya lulus — kode keluar
    # merah tanpa satu pun assertion gagal.
    BRU_SANDBOX=developer run_folders 500 "${ANSWERS_FOLDERS[@]}"
fi

if [ "${#REDIS_DOWN_FOLDERS[@]}" -gt 0 ]; then
    stop_app
    # Port 1 tidak pernah mendengarkan: koneksi ditolak seketika, jadi
    # Notifier berstatus `unavailable` (diaktifkan, tidak terjangkau).
    start_app true REDIS_ENABLED=true REDIS_URL=redis://127.0.0.1:1/0
    echo "==> bru run redis-down (Redis tidak terjangkau; disconnect bukan cancel)"
    BRU_SANDBOX=developer run_folders 500 "${REDIS_DOWN_FOLDERS[@]}"
fi

if [ "${#CRASH_FOLDERS[@]}" -gt 0 ]; then
    # Lease 6 s / heartbeat 2 s / reaper 2 s supaya attempt yang ditinggalkan
    # ditemukan dalam hitungan detik, bukan satu menit. K1 (heartbeat × 3 ≤
    # lease, ditegakkan config) dan K2 (reaper ≤ lease / 2) tetap terpenuhi.
    for folder in "${CRASH_FOLDERS[@]}"; do
        case "$folder" in
            crash-recovery | crash-cancelled) seam=(LOCAL_CRASH_AFTER_EXTERNAL_CALL=1) ;;
            # = NODE_ATTEMPT_CAP (runtime.md §1): setiap attempt yang diizinkan
            # ditinggalkan, jadi yang teruji adalah batasnya, bukan pemulihannya.
            crash-exhausted) seam=(LOCAL_CRASH_AFTER_EXTERNAL_CALL=3) ;;
            # TTL < lease: `expires_at` lewat sebelum lease, jadi reaper menutup
            # job `Expired` saat attempt-nya masih `Running`.
            crash-expired) seam=(LOCAL_CRASH_AFTER_EXTERNAL_CALL=1 JOB_TTL_RUNNING_SECS=4) ;;
            # Setiap klaim error sebelum admisi; TTL 24 s memberi ±2 siklus
            # lease-hilang sebelum deadline asli (lihat worker-error/job-state).
            worker-error) seam=(LOCAL_WORKER_ERROR_BEFORE_ADMISSION=1000 JOB_TTL_RUNNING_SECS=24) ;;
        esac
        stop_app
        start_app true \
            "${seam[@]}" \
            WORKER_LEASE_DURATION_SECS=6 \
            WORKER_LEASE_HEARTBEAT_INTERVAL_SECS=2 \
            REAPER_INTERVAL_SECS=2
        echo "==> bru run $folder (${seam[*]})"
        BRU_SANDBOX=developer run_folders 500 "$folder"
    done
fi

if [ "${#PLANNER_FOLDERS[@]}" -gt 0 ]; then
for folder in "${PLANNER_FOLDERS[@]}"; do
    stop_app
    PLANNER_MODE="${folder%%/*}"
    PLANNER_EXPECTED_FAILURE=""
    PLANNER_EXPECTED_ATTEMPTS=0
    BRU_ENVIRONMENT=local
    case "$PLANNER_MODE" in
        planner-manual-success)
            BRU_ENVIRONMENT=planner-manual
            ANSWERS_ORACLE_PREFIX=savings_deposit_total "$ROOT/scripts/answer-expectations.sh"
            start_app true LLM_CHAT_COMPLETIONS_URL="${PLANNER_PROVIDER_URL:-$LLM_CHAT_COMPLETIONS_URL}" \
                LLM_PROVIDER="${PLANNER_PROVIDER:-$LLM_PROVIDER}" LLM_MODEL="${PLANNER_MODEL:-$LLM_MODEL}"
            target="$folder"
            ;;
        planner-manual-unavailable)
            BRU_ENVIRONMENT=planner-manual
            start_app true LLM_CHAT_COMPLETIONS_URL=http://127.0.0.1:1/v1/chat/completions \
                LLM_MAX_RETRIES=0 LLM_TIMEOUT_MS=500
            target="$folder"
            ;;
        planner)
            ANSWERS_ORACLE_PREFIX=savings_deposit_total "$ROOT/scripts/answer-expectations.sh"
            start_app true LLM_CHAT_COMPLETIONS_URL="${PLANNER_PROVIDER_URL:-$LLM_CHAT_COMPLETIONS_URL}" \
                LLM_PROVIDER="${PLANNER_PROVIDER:-$LLM_PROVIDER}" LLM_MODEL="${PLANNER_MODEL:-$LLM_MODEL}"
            target="$folder"
            ;;
        planner-unavailable)
            PLANNER_EXPECTED_FAILURE=planner_unavailable
            PLANNER_EXPECTED_ATTEMPTS=2
            start_app true LLM_CHAT_COMPLETIONS_URL=http://127.0.0.1:1/v1/chat/completions LLM_MAX_RETRIES=1 LLM_TIMEOUT_MS=500
            target=planner
            ;;
        planner-invalid | planner-repair | planner-no-fit | planner-budget)
            provider_port=$((PORT + 10000))
            if lsof -nP -iTCP:"$provider_port" -sTCP:LISTEN >/dev/null 2>&1; then
                echo "Planner fixture port $provider_port is already occupied" >&2
                exit 1
            fi
            mode=invalid
            PLANNER_EXPECTED_FAILURE=planner_output_invalid
            PLANNER_EXPECTED_ATTEMPTS=2
            extra=(LLM_MAX_RETRIES=1)
            case "$PLANNER_MODE" in
                planner-repair) mode=repair; PLANNER_EXPECTED_FAILURE=planner_abstained ;;
                planner-no-fit) mode=no-fit; PLANNER_EXPECTED_FAILURE=planner_no_fit; PLANNER_EXPECTED_ATTEMPTS=1 ;;
                planner-budget) extra=(LLM_MAX_RETRIES=1 BUDGET_MAX_MODEL_CALLS=1); PLANNER_EXPECTED_ATTEMPTS=1 ;;
            esac
            python3 "$ROOT/tests/planner_provider.py" --port "$provider_port" --mode "$mode" &
            PROVIDER_PID=$!
            ready=false
            for attempt in $(seq 1 40); do
                if curl -fsS "http://127.0.0.1:$provider_port/" >/dev/null 2>&1; then ready=true; break; fi
                sleep 0.1
            done
            [ "$ready" = true ] || { echo "Planner provider fixture did not start" >&2; exit 1; }
            start_app true LLM_PROVIDER=planner-test-fixture LLM_MODEL=planner-test-fixture \
                LLM_CHAT_COMPLETIONS_URL="http://127.0.0.1:$provider_port/v1/chat/completions" \
                "${extra[@]}"
            target=planner
            ;;
        *) echo "Unknown planner stage $PLANNER_MODE" >&2; exit 1 ;;
    esac
    BRU_SANDBOX=developer run_folders 500 "$target"
    stop_app
    if [ -n "$PROVIDER_PID" ]; then
        kill -TERM "$PROVIDER_PID"
        wait "$PROVIDER_PID" 2>/dev/null || true
        PROVIDER_PID=""
    fi
done
fi

# OVR-6.7 — nol pelanggaran isolasi commit di seluruh tahap yang dijalankan.
if grep -q "commit_isolation_violation" "$LOG"; then
    echo "OVR-6.7: panggilan eksternal saat transaksi commit terbuka (I1), lihat $LOG:" >&2
    grep "commit_isolation_violation" "$LOG" >&2
    exit 1
fi
echo "==> OVR-6.7: nol pelanggaran isolasi commit (I1) di $LOG"
