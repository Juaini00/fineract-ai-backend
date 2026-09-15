# Parameter runtime operasional

**Status: NILAI AWAL, bukan hasil tuning.** Tanggal: 2026-09-13.

Dokumen ini mengumpulkan angka-angka yang sengaja ditunda ke "dokumen operasional" pada keputusan database #1–#15 ([carry-over.md](../migration/carry-over.md)) dan memberi masing-masing satu nilai awal yang dapat dijalankan. Setiap angka adalah **titik start yang beralasan, bukan hasil pengukuran**; sebagian besar hanya dapat difinalkan setelah uji kapasitas terhadap data Fineract nyata (§9).

Yang penting bukan ketepatan angka awalnya, melainkan bahwa (a) setiap angka punya dasar nyata, (b) setiap angka punya **pemicu revisi yang terukur**, dan (c) angka-angka itu **tidak saling bertentangan** — §10 mendaftar setiap konflik yang ditemukan.

Konteks produk yang mendasari semua nilai: Jarvis adalah **alat admin internal** di atas Apache Fineract, **read-only**, puluhan–ratusan job/hari, bukan skala konsumen. Mengulang node **aman** (#13) — biayanya hanya belanja query/model ganda. Karena itu nilai di bawah cenderung memilih **keamanan dan kesederhanaan di atas penghematan**.

Referensi terbukti dari repo lama (`ai_report/.env.example`): `FINERACT_DATABASE_QUERY_TIMEOUT_MS=3000`, `LLM_TIMEOUT_MS=30000`, `LLM_MAX_RETRIES=3`, `LLM_MAX_OUTPUT_TOKENS=2000`, `DEEPSEEK_MAX_INPUT_TOKENS=16000`, `JWT_ACCESS_TOKEN_EXPIRY_SECONDS=900`.

---

## 1. Worker lease & recovery (#13, #2)

| Parameter | Nilai awal | Alasan | Pemicu revisi |
| --- | --- | --- | --- |
| `WORKER_LEASE_DURATION` | **60 s** (rentang 45–180) | Lease harus lebih lama dari jeda terpanjang antar-heartbeat yang normal, **bukan** dari runtime node — dengan syarat heartbeat dipancarkan dari task terpisah (baris berikut). 60 s = toleransi 5 heartbeat hilang + clock skew. Terlalu pendek: worker sehat direbut saat GC/IO spike. Terlalu panjang: job crash menganggur lebih lama. | p99 jeda antar-`heartbeat_at` > **20 s** (⅓ lease) selama 7 hari; atau >1 kejadian/minggu `Abandoned` pada worker yang ternyata hidup |
| `WORKER_LEASE_HEARTBEAT_INTERVAL` | **10 s** | Cukup sering terhadap lease, cukup jarang agar churn tulis `chat_jobs` wajar: job 5 menit = 30 versi baris, separuh asumsi yang dipakai #1 saat menolak pemisahan `job_leases`. | `n_tup_hot_upd / n_tup_upd` < **0.80** atau `n_dead_tup/n_live_tup` > **20%** → lihat K17 |
| Rasio lease : heartbeat | **6 : 1** | Tiga adalah minimum umum; enam dipilih karena merebut lease dari worker sehat **membuang** kerja (attempt baru, `Abandoned`, belanja query ganda), sedangkan menunggu 60 s ekstra pada alat internal tidak merugikan siapa pun. Menyerap satu siklus reaper penuh. | rasio efektif (`lease / p99 jeda heartbeat`) < **3** |
| **Sumber heartbeat** | **task Tokio independen** | Prasyarat, bukan angka. Bila heartbeat hanya dipancarkan di batas node, satu node LLM worst case = 30 s × 3 retry = **90 s > lease 60 s** → worker sehat dipagari di tengah kerja. Lihat **K1**. | bila menjadi inline, lease **wajib** ≥180 s pada perubahan yang sama |
| `REAPER_INTERVAL` | **30 s** | Harus ≤ lease agar waktu deteksi total tidak melar; memberi deteksi worst case 90 s. Murah: hanya menyentuh index parsial (#1), bukan seq scan. | durasi satu putaran > **10%** intervalnya (3 s) |
| `JOB_TTL_RUNNING` | **30 menit** | Batas kerja nyata sudah dijaga budget (§6): ≈14 menit worst case. 30 menit = dua kali skenario terburuk; melewatinya berarti macet, bukan lambat. TTL adalah jaring terakhir, bukan pengganti budget. | p99 wall-time job terminal > **10 menit** (⅓ TTL) |
| `CLARIFICATION_WAIT_LIMIT` | **2 jam** (rentang 30 m – 8 j) | Dibatasi dua sisi. Bawah: manusia meninggalkan meja (rapat/makan siang 30–90 menit); batas 15 menit akan meng-`Expired` percakapan sah dan membuang hasil node yang sudah selesai. Atas: constraint "1 job nonterminal per session" (#13) berarti form yang ditinggalkan **memblokir seluruh session**; 24 jam mengunci session semalam. Tidak dibatasi umur JWT — klien me-refresh token, bukan job. | >5% form berakhir `expired` alih-alih `answered`/`skipped` dalam 30 hari (terlalu pendek); atau keluhan 409 "session terkunci" (terlalu panjang) |
| `NODE_ATTEMPT_CAP` | **3** | Hanya berguna untuk kegagalan **tak pasti/transien** (`Abandoned`, #2). Karena read-only, mengulang aman; biayanya murni budget. Tiga = percobaan awal + dua pemulihan (satu restart worker + satu blip jaringan). | >1% node mencapai attempt 3 **dan** akhirnya `Completed` (terlalu rendah); attempt 3 selalu `Failed` (turunkan ke 2) |
| `NODE_ATTEMPT_CAP_DETERMINISTIC` | **1 (tanpa retry)** | #2 sengaja memisahkan `Failed` (deterministik, diketahui) dari `Abandoned` (tak pasti). Mengulang SQL yang error sintaks/permission akan gagal identik dan hanya membakar budget. Retry hanya masuk akal untuk ketidakpastian. | muncul `failure_code` yang terbukti transien tetapi terklasifikasi deterministik → pindahkan kodenya, jangan naikkan cap global |

---

## 2. Klarifikasi (#8, K5)

| Parameter | Nilai awal | Alasan | Pemicu revisi |
| --- | --- | --- | --- |
| `RESOLVER_PAGE_SIZE` | **25** (maks. diterima 50) | clarifications.md melarang mengirim ribuan opsi. Batas atas nyata adalah manusia: daftar >25 baris tidak dipindai. Konsekuensi schema: hanya halaman yang **benar-benar dikirim** dipersist ke `clarification_options` (#8), jadi page size = unit pertumbuhan tabel berisi nama nasabah. | >20% pengguna memilih opsi dari halaman ≥2 → biasanya kualitas ranking resolver, bukan page size |
| `RESOLVER_MAX_CANDIDATES` | **500** | Resolver memuat kandidatnya utuh lalu memaginasi di memori: manifest resolver tidak mendeklarasikan parameter cursor, dan menambahkannya di aplikasi berarti mengarang predikat SQL. Angka ini adalah langit-langitnya, dan kelebihannya dinyatakan `truncated: true` pada halaman — bukan daftar yang diam-diam lebih pendek (I5). | sebuah resolver rutin menyentuh batas ini pada halaman pertama → beri resolver itu keyset SQL-side, jangan naikkan angkanya |
| `CLARIFICATION_OPTION_TTL` | **diturunkan dari `clarification_forms.expires_at`** | **Tidak boleh angka independen.** Opsi yang kedaluwarsa lebih dulu daripada form-nya membuat jawaban sah ditolak "option tidak dikenal" tanpa kesalahan pengguna. Lihat **K4**. | ukuran `clarification_options` > **1 GB** → purge saat form terminal (#8) tidak berjalan |
| `MAX_CLARIFICATION_STAGES` | **3** | clarifications.md mewajibkan membatch ambiguitas yang sudah diketahui; contoh terdokumentasi adalah dua tahap (client → account). Tiga memberi cadangan untuk kasus tiga tingkat. Stage keempat hampir pasti berarti intent tidak didukung — dan kontraknya mewajibkan melaporkan keterbatasan, bukan loop bertanya. | >2% job mencapai stage 3 **dan** `Answered` (naikkan); stage 3 selalu `Unsupported`/`expired` (turunkan ke 2) |
| `MAX_REVISIONS_PER_CLARIFICATION` | **10** | Sengaja dipisah dari batas stage karena biayanya berbeda dua orde: `refine_search` hanya memanggil resolver (**tanpa** panggilan LLM), sedangkan stage baru berarti perencanaan/komposisi model. Menyatukannya menghukum pencarian ulang yang murah. | p95 revisi per stage > **4** → perbaiki resolver, jangan naikkan batas |
| `MAX_FORMS_PER_JOB` | **20** | Plafon absolut pendeteksi loop (p95 realistis 2–4). Form kecil dan immutable, jadi 20 baris tidak berarti untuk storage — nilainya murni sebagai detektor. | ada job mencapai 20 → investigasi sebagai bug engine, bukan sinyal menaikkan batas |
| `MAX_RAW_TEXT_LENGTH` | **512** (rentang 256–1024) | `raw_text` bukan prosa melainkan istilah pencarian/nilai yang diketik (K1). Kolom identitas Fineract (`m_client.display_name`) berukuran ~100 karakter; istilah >100 karakter secara matematis tidak dapat mencocokkan kolom identitas mana pun. 512 memberi ruang untuk `typed_value` yang lebih panjang. | p99 panjang diterima > **128** → asumsi "istilah pencarian" salah, tinjau UX field |

---

## 3. Idempotency (#9)

| Parameter | Nilai awal | Alasan | Pemicu revisi |
| --- | --- | --- | --- |
| `IDEMPOTENCY_TTL` | **24 jam** (rentang 6 j – 7 h) | Tiga masalah #9 punya horizon berbeda: klik ganda = detik, retry proxy = detik–menit, **retry manual pengguna setelah balasan hilang = jam**. Yang ketiga menentukan. Batas bawah keras: **≥ `CLARIFICATION_WAIT_LIMIT`** (**K6**). | retry sah ditolak/dieksekusi ulang karena baris sudah dipurge (job duplikat dengan `request_fingerprint` identik) |
| `MAX_IDEMPOTENCY_KEY_LENGTH` | **255** (minimum **16**) | Kunci masuk UNIQUE btree; batas entri btree PostgreSQL ≈ **2704 byte**. 255 karakter UTF-8 ≤ 1020 byte → aman dengan margin besar. Jauh di bawah batas header proxy umum. Minimum 16 ditegakkan karena kunci dibuat klien: entropinya harus diwajibkan. | klien ditolak karena >255 → hampir pasti bug klien (payload di kunci) |
| `IDEMPOTENCY_BODY_EARLY_PURGE` | **false** | Acknowledgement sangat kecil (job_id + lifecycle), jadi tidak ada yang dihemat; sedangkan baris tanpa acknowledgement membuat retry sah tidak dapat di-replay `job_id` yang sama — padahal #9 mewajibkannya agar frontend tidak kehilangan handle. Menukar nol byte dengan satu kelas bug. | p95 `response_body_json` > **1 KB** → tinjau isinya; bila ber-PII, purge body pada 1 jam dan pertahankan baris sampai TTL penuh |

---

## 4. Datasets (#11)

Kelompok ini **paling eksplisit menunggu pengukuran** (#11: "angka pasti ditetapkan setelah uji kapasitas").

### Ukuran chunk — penalaran dari page/TOAST PostgreSQL

`CHUNK_ROWS = 1.000` dengan **plafon 1 MB**; chunk ditutup pada yang tercapai lebih dulu. Aturan dua dimensi diperlukan karena lebar baris Fineract bervariasi (ringkasan office ≈100 B; daftar nasabah dengan nama/nomor/saldo/tanggal ≈200–400 B).

- **Batas bawah dari overhead per-baris dan TOAST.** Page = 8 KB; nilai di atas `TOAST_TUPLE_THRESHOLD` (≈2 KB) dikompresi dan dipindah ke tabel TOAST dalam potongan ~2 KB. Chunk terlalu kecil = banyak baris, banyak overhead tuple, **tanpa keuntungan** karena chunk selalu dibaca utuh.
- **Batas atas karena JSONB tidak dapat dibaca sebagian.** Membaca satu kolom JSONB ter-TOAST mengambil **semua** potongannya lalu mendekompresi. Chunk 50 MB = 50 MB dibaca untuk menyajikan satu halaman UI.
- **Kasus pagination memutuskan sisanya.** Tabel dashboard menampilkan 25–100 baris/halaman; chunk 1.000 baris melayani **10–40 halaman UI per satu pembacaan**, sementara dataset 100.000 baris tetap hanya 100 baris `dataset_chunks`.
- Pada 1.000 × ~300 B ≈ **300 KB/chunk** — jauh di atas ambang overhead, jauh di bawah ambang de-TOAST mahal. Karena chunk pasti ter-TOAST, kompresi sudah berjalan sejak awal; ini memperkuat alasan #11 bahwa pindah ke `BYTEA` terkompresi kelak adalah **optimasi, bukan koreksi**.

| Parameter | Nilai awal | Pemicu revisi |
| --- | --- | --- |
| `CHUNK_ROWS` | **1.000** | p95 baca satu chunk > **50 ms** atau p95 `byte_size` > **1 MB** → turunkan ke 500 |
| `CHUNK_MAX_BYTES` | **1 MB** | rasio kompresi TOAST < 2× pada payload nyata → pertimbangkan jalur `BYTEA` (`format`/`encoding_version` sudah disiapkan #11) |
| `DATASET_MAX_ROWS` | **100.000** → `truncated=true` + reason | Batas **makna**, bukan teknis: dataset sebesar itu tidak dibaca manusia dan **tidak boleh disaring ulang** oleh engine (#11). Bila jawaban butuh lebih, pertanyaannya seharusnya agregasi di query sumber. **Wajib diuji ulang** terhadap ukuran nyata `m_client`/`m_savings_account`. >1% dataset menyentuh batas/bulan → tambah capability agregat, jangan naikkan batas |
| `DATASET_MAX_BYTES` | **64 MB** | RSS worker melonjak >2× saat dataset besar → engine memuat lebih dari satu chunk; perbaiki implementasi dulu |
| `JOB_RETAINED_BYTES_CAP` | **256 MB** | Lihat **K16** — resolusinya adalah verifikasi retensi saat plan, bukan sekadar menaikkan cap |
| `SESSION_RETAINED_BYTES_QUOTA` | **1 GB**, eviction LRU | Praktis tidak pernah tercapai pada volume ini: nilainya sebagai **detektor kebocoran purge**, bukan alat penjatahan. Eviction **dilarang** menyentuh dataset milik job nonterminal berapa pun umurnya (#11). Eviction pernah berjalan → selidiki reaper dulu |
| `DATASET_TTL` | **24 jam** | Menopang pagination tabel yang sedang dilihat dan follow-up dalam session kerja yang sama — satu hari kerja adalah unit alaminya. Batas bawah keras: **≥ `CLARIFICATION_WAIT_LIMIT` + `JOB_TTL_RUNNING`** (**K5**) |

---

## 5. Events & SSE (#3)

| Parameter | Nilai awal | Alasan | Pemicu revisi |
| --- | --- | --- | --- |
| `EVENT_REPLAY_RETENTION` | **30 hari** | Kebutuhan fungsional diukur dalam menit (reconnect), tetapi volumenya membuat kemurahan hati gratis: ~200 job/hari × ~40 event × ~1 KB ≈ **8 MB/hari**, dipurge lewat BRIN (#3). 30 hari memungkinkan rekonstruksi pasca-insiden **tanpa** menyentuh audit. Batas bawah keras: **K7**. | ukuran `job_events` > **10 GB** atau purge > **10 s** → jalankan partisi (#3), jangan potong retensi dulu |
| `EVENT_INLINE_PAYLOAD_SOFT` | **2 KB** | = `TOAST_TUPLE_THRESHOLD`: menjaga baris event tetap in-page pada tabel dengan **volume tulis tertinggi** (#3) — tidak ada tulis TOAST di jalur terpanas. | proporsi `job_events` ter-TOAST > **10%** → turunkan |
| `EVENT_INLINE_PAYLOAD_HARD` | **8 KB** → `payload_truncated` | = satu page PostgreSQL, sekaligus ordo `proxy_buffer_size` nginx (4–8 KB) sehingga satu frame SSE tidak dipecah buffer proxy. Di atasnya klien mengambil lewat endpoint — murah karena referensi bertipe sudah menjadi **kolom** (#3), sehingga ambang ini **dapat diubah tanpa migrasi**. | >5% event `payload_truncated` → naikkan soft ke 8 KB |
| `SSE_FALLBACK_POLL_INTERVAL` | **2 s**, backoff ×2 → maks **15 s** | Aktif hanya saat notifikasi Redis gagal. Query polling adalah index scan langsung pada PK `(job_id, sequence > cursor)` — sangat murah. Batas bawah dari persepsi manusia (<1 s tak terbaca); atas menjaga UI tidak terasa mati. | p95 keterlambatan saat Redis down > **5 s**; atau beban polling > **5%** total query |
| `SSE_FALLBACK_POLL_CAP` | **dibatasi `expires_at` job** | Cap numerik tersendiri menciptakan mode gagal senyap: polling berhenti padahal job berjalan. Batas yang benar sudah durable — TTL job. | koneksi SSE aktif yang job-nya sudah terminal > **0** (melanggar sse.md butir 6) |
| `SSE_TRANSPORT_COMMENT_INTERVAL` | **15 s** (rentang 10–25) | Musuhnya idle timeout jaringan, bukan pengguna: nginx `proxy_read_timeout` 60 s, AWS ALB 60 s, Cloudflare ~100 s. 15 s memberi **4 kesempatan** sebelum 60 s. Aturan: **< min(idle timeout seluruh hop) / 3**. Tidak disimpan (#3) dan **tidak pernah** bukti worker hidup. | pemutusan SSE berkorelasi idle ≈60 s di proxy produksi → turunkan ke 10 s **dan** verifikasi `proxy_buffering off` |
| `SSE_OUTGOING_BUFFER` | **64 frame / 256 KB** per koneksi | Diturunkan dari ambang payload, bukan dipilih sendiri (64 × ~4 KB). Dengan puluhan stream, worst case ≈16 MB — dapat dipertanggungjawabkan. Overflow ⇒ putuskan klien, biarkan replay (sse.md). | >0 pemutusan klien-lambat/hari pada jaringan sehat → naikkan ke 128 frame; memori linear terhadap stream hingga >100 MB → turunkan |

---

## 6. Responses, plans, budget (#10, #12, #1)

| Parameter | Nilai awal | Alasan | Pemicu revisi |
| --- | --- | --- | --- |
| `MAX_BLOCKS_JSON_BYTES` | **256 KB** (praktis 8–64 KB) | Tabel besar **tidak** disalin ke blok — blok tabel merujuk handle (#10). Batas produksi konten: `LLM_MAX_OUTPUT_TOKENS=2000` ≈ 8 KB teks. Melewatinya berarti ada tabel yang bocor inline: batas ini **penegak aturan**, bukan penghemat storage. Pelanggaran ⇒ `validation_status='failed'` + versi `limitation` yang lebih konservatif (#10). | >0 response mencapai batas → selidiki sebagai bug inline, jangan naikkan |
| `RESPONSE_RETENTION` | **90 hari** atau CASCADE session | Horizon investigasi praktis untuk "jawaban sukses yang salah" (PRD §10): keluhan datang dalam hari–minggu. Jejak permanen tetap hidup lewat `response_hash` di audit. | permintaan investigasi atas response yang sudah dipurge > **1×/kuartal** → 180 hari |
| `FAILED_RESPONSE_EARLY_PURGE` | **false** | #10 menyimpan versi gagal justru **sebagai bahan investigasi**, dan audit hanya menyimpan hash. Memurge lebih cepat menghapus satu-satunya artefak yang menunjukkan **apa** yang salah dikatakan sistem, demi kilobyte. | proporsi `validation_status='failed'` > **5%** → itu insiden kualitas komposisi, bukan pemicu tuning retensi |
| `PLAN_VERSION_RETENTION` | **mengikuti job** | Maksimum 3 baris/job, dokumen JSONB kecil. Aturan retensi tersendiri untuk beberapa kilobyte adalah kompleksitas tanpa imbalan, dan memurge plan lama merusak persis apa yang dibeli #12: reproducibility eksak. | ukuran `job_plans` > **1 GB** |
| `MAX_REPLAN_PER_JOB` | **2** (plan_version maks 3) | Tiap re-plan = siklus perencanaan + verifikasi (2 panggilan model). Satu re-plan menutup "fakta live mengubah rencana" dan `change_intent` (K1). Re-plan ketiga hampir selalu berarti tidak didukung. | >2% job mencapai 2 **dan** `Answered` (naikkan); mencapai 2 lalu `Unsupported` (turunkan ke 1) |
| `BUDGET_MAX_QUERIES` | **30** | Diturunkan dari struktur plan (1–20 node, sebagian probe + query utama, plus retry). Cek waktu: 30 × 3–15 s = 1,5–7,5 menit, konsisten dengan TTL 30 menit. Counter **absolut lintas plan version** (#1). | p95 `query_count` job sukses > **10** → periksa reuse output (#2 `input_binding_hash`) sebelum menaikkan |
| `BUDGET_MAX_MODEL_CALLS` | **12** (menghitung retry) | Dijumlahkan dari peran nyata: intent 2 + plan 1 + 2 re-plan × 2 + klarifikasi ≈3 + compose 1 + validate 1. Cek waktu: 12 × 30 s = 6 menit. Lihat **K8**. | p95 > **6** → engine memanggil model di tempat yang seharusnya deterministik; itu bug arsitektur |
| `BUDGET_MAX_TOKENS` | **250.000** | Diturunkan dari konfigurasi terbukti: (16.000 input + 2.000 output) × 12 = 216.000, margin ~15%. Budget token **tidak boleh mengikat sebelum** budget panggilan. | job berhenti karena token **sebelum** mencapai cap panggilan > 0 → ukur ukuran prompt |
| `BUDGET_MAX_COST` | **UNSET** | **Turunan, bukan pilihan**: = cap token × harga provider. Provider/model masih blocking selection di tech-stack, jadi menuliskan angka sekarang berarti mengarang. **Aturan yang dikunci**: cap biaya selalu diturunkan dari cap token + tabel harga, tidak pernah dikonfigurasi independen. | provider dikunci → hitung; tarif berubah >20% → hitung ulang |
| `PROBE_QUERY_TIMEOUT` | **3 s** | Presedens terbukti (`FINERACT_DATABASE_QUERY_TIMEOUT_MS=3000`). Probe dirancang cepat dan selektif. | p95 durasi probe > **1 s** |
| `ANALYTICAL_QUERY_TIMEOUT` | **15 s** | 3 detik **tidak cukup** untuk query beragregasi di atas tabel Fineract produksi; satu angka untuk dua kelas query adalah kesalahan yang bisa diprediksi sekarang. 15 s menjaga 30 query worst case tetap di dalam TTL. | capability approved dengan p95 > **7 s** → optimalkan SQL/index di sumber, jangan naikkan timeout |
| `NODE_OUTPUT_INLINE_THRESHOLD` | **32 KB atau 200 baris** | Ledger tidak setinggi-tulis `job_events`, sehingga TOAST di sini murah dan ambang 2 KB terlalu galak (memaksa setiap hasil sedang menjadi dataset penuh siklus hidup). 32 KB ≈ 100–150 baris Fineract khas — cukup untuk probe, agregat, dan top-N yang **tidak perlu** pagination. | p95 `output_json` > **16 KB**; atau ada blok tabel berpaginasi yang sumbernya inline (pelanggaran #11 aturan 3) |

---

## 7. Konfigurasi global (#15)

| Parameter | Nilai awal | Alasan |
| --- | --- | --- |
| `pii.enabled` | **false** (fail closed) | Arah default wajib ke sisi aman. Baris konfigurasi hilang/korup diperlakukan sama dengan `false`. |
| `pii.mode` | **`withhold`** | Kolom identitas ditahan, bukan dimasking — pola unik pada string termasking masih dapat membocorkan identitas, dan jawaban agregat tetap utuh. `mask` dapat ditambahkan kelak tanpa mengubah schema. |

Nilai efektif **wajib** disnapshot ke `chat_jobs.scope_json` bersama `setting_version` saat job diterima (#15 aturan 1). Tanpa cache: satu baris ter-index dibaca saat accept.

---

## 8. Matriks retensi

> **AUDIT hidup lebih lama daripada segalanya dan TIDAK PERNAH ikut CASCADE bersama session.** Audit menyimpan metadata/provenance/hash — **bukan** baris data mentah dan bukan isi response. Justru karena isinya dihapus lebih dulu, jejak audit harus tetap ada agar "apa yang terjadi" masih terjawab setelah "apa isinya" lenyap.
>
> Urutan wajib: `audit` ≥ `job & response` ≥ `node ledger` ≥ `events` ≥ `datasets` ≥ `TTL runtime`

| Entitas | Retensi awal | Pemilik penghapusan | Catatan |
| --- | --- | --- | --- |
| **Audit** | **2 tahun** (usulan; **wajib konfirmasi compliance**) | purge terjadwal; **tidak pernah** CASCADE | Satu-satunya angka di dokumen ini yang **bukan keputusan engineering**. Jarvis beroperasi di atas core perbankan; klasifikasi dokumennya harus diputuskan legal/compliance. Sampai ada keputusan, **jangan turunkan**. |
| Audit evidence | lebih pendek dari audit | reaper | Purge = `payload_json := NULL, purged_at := now()`; baris tetap ada sebagai bukti (#4) |
| Job (`chat_jobs`) | 90 hari / CASCADE session | CASCADE | `request_json`/`request_text` **tidak** dipurge lebih awal: itu menghapus persis properti self-contained yang membuatnya ditaruh di job (#1). Perlindungan PII yang benar adalah CASCADE session + audit ber-hash. |
| Node ledger | mengikuti job | CASCADE | Ledger tanpa job tidak dapat ditafsirkan; job tanpa ledger kehilangan posisi eksekusi (#1 sengaja tidak menyimpannya sebagai kolom) |
| Events | **30 hari** | purge usia via BRIN | Lebih pendek dari job, dan itu **bukan** konflik: sse.md butir 5 menetapkan jalur "cursor kedaluwarsa ⇒ snapshot baru", dan riwayat yang hilang tidak pernah disembunyikan |
| Responses | 90 hari / CASCADE session | CASCADE | Audit mempertahankan `response_hash` |
| Datasets + chunks | **24 jam** | TTL + reaper; CASCADE | Paling pendek karena paling besar dan paling penuh PII baris mentah |
| Clarification forms/answers | mengikuti job | CASCADE | Bukti "apa yang dilihat pengguna saat menjawab" (#8) |
| Clarification options | **purge saat form terminal/kedaluwarsa** | reaper | Memuat nama nasabah; audit menyimpan **referensi** option-set, bukan label mentah | 
| Idempotency keys | **24 jam** | reaper | Tidak menyimpan data analisis |
| Plans | mengikuti job | CASCADE | |
| `exchange_rates` | **permanen** | — | Append-only, volume kecil; kurs lama dibutuhkan untuk mereproduksi analisis lama (#14) |
| `system_settings` | **permanen** | — | Append-only; ia **adalah** riwayat konfigurasinya (#15) |
| Katalog pengetahuan | N versi terbaru **+ semua yang masih dirujuk** | purge terjadwal | Append-only; versi yang dirujuk `job_plans`/`job_node_runs` tidak boleh pernah dihapus (#7) |

---

## 9. Angka yang TIDAK dapat difinalkan tanpa uji kapasitas

Dipisahkan agar dokumen ini tidak terbaca sebagai hasil tuning.

| Parameter | Apa yang harus diukur | Terhadap apa |
| --- | --- | --- |
| `CHUNK_ROWS`, `CHUNK_MAX_BYTES` | median & p95 **lebar baris dalam byte JSON** untuk 10 capability tersering; rasio kompresi TOAST aktual; latensi baca chunk | snapshot Fineract berukuran produksi, **bukan** data demo |
| `DATASET_MAX_ROWS/BYTES` | distribusi `row_count` per capability; persentase yang menyentuh batas | sama, plus daftar pertanyaan nyata admin |
| `JOB_RETAINED_BYTES_CAP`, `SESSION_RETAINED_BYTES_QUOTA` | jumlah dataset retained per job nyata dan total byte per session per hari kerja | beban harian nyata setelah rilis terbatas |
| JSONB vs BYTEA terkompresi | ukuran on-disk, latensi tulis, latensi baca chunk untuk kedua format pada payload identik | #11 menyatakan keputusan final menunggu uji kapasitas |
| `ANALYTICAL_QUERY_TIMEOUT` | p95/p99 runtime **setiap SQL approved** | Fineract produksi dengan index nyata |
| `BUDGET_MAX_TOKENS`, `BUDGET_MAX_COST` | token in/out aktual per panggilan planner/composer atas ≥50 pertanyaan admin nyata | provider/model yang sudah dikunci |
| `WORKER_LEASE_DURATION`, `HEARTBEAT_INTERVAL` | p95/p99 **runtime per node** menurut `node_kind`; p99 jeda antar-heartbeat | worker pada resource deployment nyata |
| `JOB_TTL_RUNNING` | p95/p99 wall-time job terminal | sama |
| `SSE_OUTGOING_BUFFER`, `SSE_TRANSPORT_COMMENT_INTERVAL` | stream bersamaan pada puncak; **idle timeout aktual setiap hop proxy/LB produksi** | sse.md mewajibkan verifikasi flush melalui proxy nyata — bukan opsional |
| `CLARIFICATION_WAIT_LIMIT` | distribusi waktu balas manusia (`clarification.required` → jawaban diterima) | rilis terbatas; **tidak dapat disimulasikan** |
| Retensi audit | klasifikasi dokumen dari legal/compliance | bukan pengukuran — keputusan kebijakan |

---

## 10. Pemeriksaan konsistensi

Setiap baris adalah pasangan parameter yang **akan** saling merusak bila dipilih terpisah.

**K1 — lease vs runtime node worst case.** 60 s < 30 s × 3 retry = **90 s**. Bila heartbeat dipancarkan di batas node, lease worker sehat kedaluwarsa di tengah node → job direbut, attempt `Abandoned`, kerja diulang tanpa sebab — kelas kegagalan yang #13 dirancang mencegah. **Resolusi: heartbeat wajib task Tokio independen**; bila tidak memungkinkan, lease ≥180 s pada perubahan yang sama. Kedua angka tidak boleh dipilih terpisah oleh orang berbeda. ✅ *ditutup sebagai prasyarat arsitektur.*

**K2 — reaper vs lease.** Invariant `REAPER_INTERVAL ≤ LEASE_DURATION / 2`. ✅ 30 ≤ 30.

**K3 — satu kolom `expires_at`, dua TTL.** ✅ *ditutup oleh amandemen #1*: `expires_at` dihitung ulang pada setiap transisi fase.

**K4 — TTL option vs batas tunggu.** ✅ `clarification_options.expires_at` diturunkan dari form, bukan dikonfigurasi sendiri.

**K5 — TTL dataset vs job menunggu.** Invariant `DATASET_TTL ≥ CLARIFICATION_WAIT_LIMIT + JOB_TTL_RUNNING`. ✅ 24 j ≥ 2,5 j — **margin ini hilang bila batas tunggu dinaikkan ke 8–24 jam**, salah satu revisi paling mungkin. Terkait: eviction kuota **dilarang** menyentuh dataset milik job nonterminal (#11).

**K6 — TTL idempotency vs batas tunggu.** Invariant `IDEMPOTENCY_TTL ≥ CLARIFICATION_WAIT_LIMIT`. ✅ 24 j ≥ 2 j.

**K7 — replay SSE vs batas tunggu.** Invariant `EVENT_REPLAY_RETENTION ≥ CLARIFICATION_WAIT_LIMIT + JOB_TTL_RUNNING`. ✅ 30 hari ≫ 2,5 jam.

**K8 — budget × timeout vs TTL job.** 12 × 30 s + 30 × 15 s ≈ **14 menit** vs TTL 30 menit ✅. **Tetapi** bila `NODE_ATTEMPT_CAP` diterapkan **di atas** cap 12, worst case ≈18 menit dan margin habis. **Resolusi: `model_call_count` menghitung retry; cap 12 bersifat absolut**, bukan "12 yang sukses".

**K9 — attempt cap × replan cap.** 3 × 3 = 9 eksekusi teoretis node "yang sama". Aman **karena desain, bukan angka**: #1 menaruh counter budget di **baris job**, bukan per plan. Pertahankan properti ini — memindahkan budget ke per-plan akan diam-diam melipattigakan plafon.

**K10 — payload SSE vs `blocks_json`.** 2–8 KB vs 256 KB konsisten **hanya bila** `job.completed` tidak pernah membawa dokumen inline dan selalu membawa `response_version` sebagai referensi (#3 sudah menjadikannya kolom). Dinyatakan eksplisit agar tidak ada yang menyematkan dokumen 256 KB ke satu frame SSE.

**K11 — buffer keluar vs ambang payload.** 256 KB diturunkan dari 64 × ~4 KB. Menaikkan ambang payload tanpa menaikkan buffer menurunkan jumlah frame efektif **secara diam-diam**. Keduanya **harus diubah bersama**.

**K12 — dua parameter bernama "heartbeat".** `WORKER_LEASE_HEARTBEAT_INTERVAL` (worker → PostgreSQL, **bukti worker hidup**) vs `SSE_TRANSPORT_COMMENT_INTERVAL` (server → browser, **hanya bukti koneksi hidup**). Nilai berbeda dan **tidak boleh "diselaraskan"** oleh siapa pun yang mengira keduanya hal yang sama.

**K13 — TTL dataset vs retensi response vs D01.** ✅ *ditutup oleh #14*: `published_reports` ditunda; laporan terbit kelak **wajib self-contained** (blok tabel inline, tanpa handle). Response biasa merujuk handle dan setelah TTL menyatakan "detail data sudah kedaluwarsa" (#11 aturan 6). **Tidak ada kelas dataset `pinned`.**

**K14 — panjang kunci vs batas btree.** 255 karakter ≤ 1020 B ≪ ≈2704 B ✅. Dicatat agar batas ini tidak pernah dinaikkan ke 4096 "karena header HTTP memungkinkan" — itu membuat `INSERT` gagal pada UNIQUE index, bukan ditolak validasi.

**K15 — page size × batas revisi.** 25 × 10 = hingga **250 baris `clarification_options` per form**, semuanya berpotensi memuat nama nasabah. Konsisten hanya karena batas revisi ada **dan** #8 mem-purge saat form terminal. Invariant praktis: `RESOLVER_PAGE_SIZE × MAX_REVISIONS ≤ 500`.

**K16 — cap byte per job vs jumlah node.** ✅ *ditutup oleh amandemen #1*: kebutuhan retensi per node dinyatakan saat verifikasi plan; plan yang melebihi ditolak **sebelum** dijalankan.

**K17 — `fillfactor = 85` vs interval heartbeat.** Ruang bebas 15% dibagi seluruh baris dalam satu page; dengan baris `chat_jobs` yang lebar, sisa ~1,2 KB hanya menampung beberapa versi baris sebelum HOT chain putus. **Harus diukur**, tidak bisa ditebak. Pemicu: `n_tup_hot_upd / n_tup_upd` < **0.80**. Respons **berurutan**: (1) `fillfactor` → 70, (2) **baru** pisahkan `job_leases`.

---

## 11. Ringkasan nilai awal

```
# Worker lease & recovery (#13, #2)
WORKER_LEASE_DURATION_SECS            = 60
WORKER_LEASE_HEARTBEAT_INTERVAL_SECS  = 10     # task Tokio independen — K1
REAPER_INTERVAL_SECS                  = 30
JOB_TTL_RUNNING_SECS                  = 1800
CLARIFICATION_WAIT_LIMIT_SECS         = 7200   # expires_at dihitung ulang — K3
NODE_ATTEMPT_CAP                      = 3      # kegagalan tak pasti (Abandoned)
NODE_ATTEMPT_CAP_DETERMINISTIC        = 1      # kegagalan diketahui (Failed)

# Klarifikasi (#8, K5)
RESOLVER_PAGE_SIZE                    = 25     # maks diterima 50
RESOLVER_MAX_CANDIDATES               = 500    # langit-langit paginasi di memori
CLARIFICATION_OPTION_TTL              = derived from form.expires_at   # K4
MAX_CLARIFICATION_STAGES              = 3
MAX_REVISIONS_PER_CLARIFICATION       = 10     # K15
MAX_FORMS_PER_JOB                     = 20
MAX_RAW_TEXT_LENGTH                   = 512

# Idempotency (#9)
IDEMPOTENCY_TTL_SECS                  = 86400  # K6
MAX_IDEMPOTENCY_KEY_LENGTH            = 255    # minimum 16 — K14
IDEMPOTENCY_BODY_EARLY_PURGE          = false

# Datasets (#11)
CHUNK_ROWS                            = 1000
CHUNK_MAX_BYTES                       = 1_048_576
DATASET_MAX_ROWS                      = 100_000
DATASET_MAX_BYTES                     = 67_108_864
JOB_RETAINED_BYTES_CAP                = 268_435_456      # K16
SESSION_RETAINED_BYTES_QUOTA          = 1_073_741_824
DATASET_TTL_SECS                      = 86400            # K5

# Events & SSE (#3)
EVENT_REPLAY_RETENTION_DAYS           = 30               # K7
EVENT_INLINE_PAYLOAD_SOFT_BYTES       = 2048
EVENT_INLINE_PAYLOAD_HARD_BYTES       = 8192             # K10
SSE_FALLBACK_POLL_INTERVAL_SECS       = 2                # backoff x2 → maks 15
SSE_FALLBACK_POLL_CAP                 = bounded by job expires_at
SSE_TRANSPORT_COMMENT_INTERVAL_SECS   = 15               # K12
SSE_OUTGOING_BUFFER_FRAMES            = 64               # K11
SSE_OUTGOING_BUFFER_BYTES             = 262_144

# Responses & plans (#10, #12)
MAX_BLOCKS_JSON_BYTES                 = 262_144          # K10
RESPONSE_RETENTION_DAYS               = 90
FAILED_RESPONSE_EARLY_PURGE           = false
PLAN_VERSION_RETENTION                = follows job
MAX_REPLAN_PER_JOB                    = 2

# Budget (#1)
BUDGET_MAX_QUERIES                    = 30
BUDGET_MAX_MODEL_CALLS                = 12               # menghitung retry — K8
BUDGET_MAX_TOKENS                     = 250_000
BUDGET_MAX_COST                       = UNSET (provider belum dikunci)
PROBE_QUERY_TIMEOUT_MS                = 3000
ANALYTICAL_QUERY_TIMEOUT_MS           = 15000

# Ledger (#2)
NODE_OUTPUT_INLINE_THRESHOLD_BYTES    = 32_768           # atau 200 baris

# Konfigurasi global (#15)
pii.enabled                           = false            # fail closed
pii.mode                              = withhold

# Retensi
AUDIT_RETENTION                       = 2 tahun (USULAN — butuh keputusan compliance)
```

**Satu hal yang harus ditutup sebelum implementasi**: **K1** — sumber heartbeat menentukan apakah lease 60 s benar. K3, K13 dan K16 sudah ditutup lewat amandemen keputusan (#1, #14).
