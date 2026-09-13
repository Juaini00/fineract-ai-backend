# Checklist kelengkapan dokumentasi Jarvis

Tanggal pemeriksaan: 2026-09-09. Basis: delapan dokumen yang ada di checkout repository baru sebelum checklist ini ditambahkan.

Pembaruan diskusi 2026-09-09: [keputusan cakupan data](product/2026-09-09-dataset-scope-decisions.md) menyimpan baseline handoff dan D01–D15; tinjauan celah fungsional (gap-review) ditutup pada D15 via audit sistematis. Checklist di bawah membedakan keputusan tercatat dari kontrak dan bukti deployment yang belum selesai.

**Status keseluruhan: belum siap implementasi.** Arah produk dan kontrak interaksi sudah tercatat; desain database, audit operasional, schema lengkap dan parameter runtime masih perlu diselesaikan.

**Target checklist: aplikasi lengkap sesuai scope yang disepakati, siap production release dan maintenance; bukan desain MVP.** Dua belas area di bawah adalah kelompok tanggung jawab release. Data besar Fineract wajib didesain dan diuji sebagai beban normal. Milestone implementasi boleh bertahap, tetapi kebutuhan wajib tidak dipindahkan ke setelah release demi menyederhanakan tahap awal. Global memory tetap deferred berdasarkan kesepakatan sebelumnya.

## Cara membaca

- `[x]` = keputusan/perilaku tersebut sudah tertulis; bukan berarti aplikasi sudah dibangun atau diuji.
- `[ ]` = dokumentasi/keputusan masih perlu dilengkapi.
- **Parsial** = file tersedia, tetapi masih menyebut detail penting sebagai open/pending.
- **Belum ada** = dokumen khusus belum tersedia; sebagian prinsip bisa sudah tercantum dalam PRD.
- **Deferred** = sengaja di luar rilis awal, bukan kewajiban implementasi yang terlupakan.

Jangan menghitung persentase kesiapan dari jumlah file atau checkbox: bobot keputusan berbeda. Checkbox ditutup hanya jika ada dokumen pemilik dan bukti keputusan yang jelas. Checklist melacak status; kontrak tetap dimiliki dokumen masing-masing.

## Inventaris dokumen

| Dokumen | Ketersediaan/status | Kekurangan utama |
| --- | --- | --- |
| [README](README.md) | Ada: indeks dan gate | Diperbarui saat dokumen baru dibuat |
| [Design review](design-review.md) | Ada: gap dan riwayat keputusan | Menutup gap sesuai hasil pembahasan |
| [PRD](product/prd.md) | Ada, parsial untuk readiness | Cakupan analisis awal dan target terukur |
| [Keputusan cakupan data](product/2026-09-09-dataset-scope-decisions.md) | Ada: baseline dan D01–D15 disepakati; gap-review ditutup | Inventaris formal, mapping sumber/deployment dan kontrak teknis belum final |
| [Tech stack](architecture/tech-stack.md) | Ada, parsial | Versi, provider/model, parser, exporter, storage |
| [Engine](architecture/engine.md) | Ada, parsial | Matriks transisi lengkap, lease/fencing, recovery |
| [API](contracts/api.md) | Ada, parsial | OpenAPI/schema lengkap, endpoint pendukung dan error matrix |
| [Klarifikasi](contracts/clarifications.md) | Ada, parsial | Schema, opsi resolver, limits dan expiry |
| [SSE](contracts/sse.md) | Ada, parsial | Payload lengkap, replay limits, auth expiry dan wire errors |
| `architecture/overview.md` | Belum ada | Diagram komponen dan ownership antarmodul |
| [Database design](data/database-design.md) | Ada: invarian, matriks koneksi, ERD, tabel, referential action, batas transaksi | Utang D1–D5 (§2.2) belum tertutup; terutama propagasi `completeness` yang menunggu `contracts/responses.md` |
| `data/analytical-contracts.md` | Belum ada | Kontrak pertama dan compiler/validation specification |
| `data/dataset-lifecycle.md` | Belum ada | Storage, snapshot, pagination dan retention |
| `architecture/memory-context.md` | Belum ada | Memory lifecycle, compaction dan budget |
| `contracts/responses.md` | Belum ada | Schema blok, evidence dan validation |
| `security/access-data-policy.md` | Belum ada | Identitas dashboard, tenant, scope dan PII |
| `operations/observability.md` | Belum ada | Audit, logs, traces, metrics dan retention |
| [Runtime](operations/runtime.md) | Ada: nilai awal + pemicu revisi + 17 pemeriksaan konsistensi | Nilai belum diukur; deployment/kapasitas/backup-restore belum ditulis; K1 belum ditutup |
| `verification/acceptance.md` | Belum ada | Matriks requirement → skenario → hasil yang diharapkan |
| [Carry-over](migration/carry-over.md) | Ada: klasifikasi tabel lama + keputusan #1–#15 + K1–K5 | Inventaris aset kode/migrasi dengan source revision belum disusun |
| `decisions/` | Belum ada | Catatan keputusan arsitektur tersendiri; keputusan kini ada di dokumen terkait |

## 1. Produk dan arsitektur

- [x] Repository backend baru; dashboard lama menyesuaikan kontrak backend.
- [x] Read-only pada Fineract, dengan penulisan state di database aplikasi.
- [x] Satu Engine; simple path dan multi-step mengikuti lifecycle yang sama.
- [x] SQL/operasi deterministik menghasilkan angka; model menjelaskan bukti.
- [x] Unsupported jika tidak ada capability/analytical contract yang disetujui.
- [x] Tiga crate dan batas route → service → repository → database tercatat.
- [ ] Diagram komponen, input/output dan pemilik setiap tahap.
- [ ] Daftar pertanyaan/operasi analisis wajib untuk rilis awal dan batasnya.
- [ ] Inventaris cakupan full release per domain/resource, termasuk arti lengkap aktivitas loan.
- [x] Baseline domain, resource penghubung dan tambahan cakupan D01–D15 tercatat dalam dokumen keputusan.
- [x] Tinjauan celah domain (gap-review) selesai pada D15 via audit sistematis seluruh permukaan Fineract.
- [ ] Inventaris dataset formal disusun dan direview; persetujuan konsep bukan finalisasi dataset.
- [ ] Target latency, throughput, kualitas jawaban dan ukuran data yang terukur.

## 2. Tech stack dan LLM

- [x] Fondasi Rust, Axum/Tower, Tokio, SQLx/PostgreSQL, Redis, Serde/Schemars, Validator, Decimal dan Tracing tercatat.
- [x] Rig sebagai arah client LLM; Engine tetap memiliki kontrol.
- [x] SQL-first dan komposisi Rust terbatas; kandidat opsional dibedakan dari stack wajib.
- [ ] Provider/model, token counting, structured output dan fallback yang terverifikasi kompatibel.
- [ ] Library YAML, SQL AST, tooling API/schema dan exporter observability final.
- [ ] Matriks versi/toolchain/features beserta alasan pemilihan.
- [ ] Parameter retry, timeout, biaya/token dan batas pemanggilan model.

## 3. Engine, scheduler dan recovery

- [x] Status job, outcome dan completeness dipisahkan.
- [x] WaitingForUser adalah suspended; response assembly berada setelah graph data.
- [x] Fan-in, bounded concurrency dan reuse output durable sudah menjadi aturan.
- [x] Satu job nonterminal per session menjadi arah awal.
- [x] Retry/re-plan berbagi budget; tidak menjanjikan exactly-once external execution.
- [ ] Matriks lengkap status/outcome, node kinds/status dan transisi yang legal.
- [ ] Worker lease, fencing, renewal dan recovery attempt yang tidak pasti.
- [ ] Race completion/cancellation/expiry dan penyelesaian node aktif.
- [ ] Aturan detail invalidasi output saat re-plan/scope/freshness berubah.

## 4. Database dan transaksi

- [x] PostgreSQL adalah sumber state durable; Redis hanya live coordination.
- [x] Kebutuhan konsistensi state, audit dan public event sudah tercatat.
- [ ] ERD dan definisi tabel/kolom relasional versus JSONB.
- [ ] Ownership, foreign key, uniqueness, indexes dan schema versioning.
- [ ] Transaksi create job, accept answer, complete node dan complete response.
- [ ] Enforcement idempotency, satu job per session, lease/fencing dan event sequence.
- [ ] Cleanup/deletion, retention relationships dan strategi migration baru.

## 5. Klarifikasi

- [x] Form bertipe: single/multiple choice, text, number, date/range dan boolean.
- [x] Radio/select/checkbox merupakan presentasi dari tipe semantik.
- [x] Conditional fields dan tahap yang membutuhkan resolver dibedakan.
- [x] Suggestions tidak auto-submit atau menebak identitas.
- [x] Job yang sama, revision, idempotency, stale answer dan field errors.
- [x] Pembatasan opsi, invalidasi jawaban turunan dan jalur no-match.
- [ ] JSON Schema lengkap: field, answer, condition, suggestion dan examples.
- [ ] Endpoint/cursor resolver opsi dan kontrak all-matches.
- [ ] Batas putaran, ukuran input/opsi, expiry dan date/number semantics final.

## 6. HTTP API dan SSE

- [x] Endpoint create/read/events/responses/cancel dan makna durable HTTP 202.
- [x] Envelope JSON, idempotency serta 409/422 untuk kasus yang dibahas.
- [x] Fase SSE nyata, status node paralel dan larangan progress palsu.
- [x] Sequence, snapshot/cursor, replay, deduplication dan disconnect ≠ cancel.
- [x] Fetch-based bearer SSE, bounded buffering dan terminal reconnect policy.
- [x] Initial release: progress dahulu, respons final tervalidasi kemudian.
- [ ] OpenAPI dan request/response/event schemas lengkap.
- [ ] Session/message/result/dataset pagination serta option-resolver endpoints.
- [ ] Full error matrix, repeat-cancel dan stale-cursor wire behavior.
- [ ] Heartbeat, replay/idempotency retention, buffer/payload limits.
- [ ] Token expiry/revocation dan proxy/Redis-outage acceptance detail.

## 7. Analisis dan hasil besar

- [x] Dua mode query, compiler terstruktur, scope/PII dan validation guards.
- [x] Grain, join duplication, preview versus analysis completeness.
- [x] Handle, bounded processing, budget gabungan dan overflow tanpa mempersempit scope diam-diam.
- [x] Kebutuhan laporan penutupan yang dapat ditampilkan kembali, atribusi office historis dan konsolidasi currency disepakati (D01–D03).
- [x] Mapping client charges, standing instructions, teller/kasir, provisioning aktual, kegiatan group/center dan audit tindakan Fineract tercatat (D04–D09).
- [x] Inventarisasi custom datatables per deployment dan analisis kelengkapan data disepakati (D10–D11).
- [x] Accounting/GL scoped (traceability, bukan laporan keuangan penuh), existing reports di luar eksekusi, scheduler job runs, dan disposisi celah audit sistematis (Surveys/PPI, credit-bureau results, dll.) disepakati (D12–D15).
- [ ] Verifikasi sumber/relasi/measure resource tambahan; bukti demo atau inventaris lama bukan verifikasi tenant aktual.
- [ ] Kontrak posisi historis versus laporan penutupan: cutoff, penerbitan/revisi, reproduksi, office history dan retention.
- [ ] Sumber/jenis/tanggal kurs, konversi, pembulatan dan perilaku kurs tidak tersedia.
- [ ] Registry/field custom datatables aktual, sensitivitas, cardinality, versioning dan schema drift.
- [ ] Semantik kosong/tidak berlaku/referensi invalid, coverage dan rincian tindak lanjut sesuai scope.
- [ ] Kontrak YAML pertama, contoh analytical spec dan SQL hasil kompilasi.
- [ ] Definisi measure: currency, rounding, null, dates, ties dan periode kosong.
- [ ] Validasi relasi/fungsi/field, catalog versioning dan retrieval/fallback.
- [ ] Physical storage, chunk schema, snapshot/freshness dan stable pagination.
- [ ] Handle lifecycle, expiry, ukuran maksimal dan apa yang tersisa untuk audit.
- [ ] Kontrak filter AND/OR bertipe, projection field/order, aggregate filter, sort dan explicit limit.
- [ ] Preservasi intent eksplisit admin dari parsing sampai SQL/response; tidak ada silent field/filter omission.
- [ ] Aktivitas loan lintas resource: normalized event, source references, stable ordering dan coverage per sumber.
- [ ] Workload Fineract realistis: volume/selectivity/concurrency dan jalur sukses data besar; fallback limit saja tidak cukup.

## 8. Memory dan context

- [x] History, job state, structured memory dan LLM working set dipisahkan.
- [x] Budget sebelum setiap model call, incremental summary dan provenance sebagai arah desain.
- [x] Otorisasi tidak dipercaya dari ringkasan; global memory deferred.
- [ ] Schema memory, promotion/invalidation, watermark dan summary versioning.
- [ ] Algoritme pemilihan konteks, token allocation dan compaction triggers.
- [ ] Recovery ringkasan gagal/salah dan stale facts.
- [ ] Kuota session/pengguna, retention, penghapusan dan pagination rinci.

## 9. Respons, temuan dan suggestion

- [x] Blok narrative, metrics, table, chart, findings, comparison, limitations dan suggestions disepakati.
- [x] Fakta versus hipotesis, angka deterministik, provenance dan fallback narasi.
- [x] Suggestion adalah follow-up yang didukung dan tidak mengeksekusi otomatis.
- [ ] ResponseDocument/block schemas, IDs, versioning dan client compatibility.
- [ ] Evidence lineage: temuan → metrik → operasi → dataset → sumber.
- [ ] Claim validation, aturan pemilihan blok dan partial-block failure.
- [ ] Suggestion payload/bindings dan contoh respons lengkap lintas skenario.
- [ ] Format/field yang diminta admin, precedence bila structured input bertentangan dengan teks, dan chart/shape compatibility.

## 10. Audit, logs, traces dan metrics

- [x] Audit mencakup diagnosis respons salah maupun job gagal.
- [x] Kebutuhan provenance, versi plan/model/prompt, attempts dan evidence tercatat.
- [x] Audit dipisahkan dari telemetry dan public SSE; data sensitif tidak masuk log umum.
- [x] Audit wajib menjadi prasyarat protected progress.
- [ ] Schema audit event per tahap, correlation IDs dan controlled evidence storage.
- [ ] Transaksi audit dan perilaku ketika write/storage gagal.
- [ ] Akses investigasi, append-only enforcement, redaction dan retention.
- [ ] Log fields/levels, spans, sampling, metrics, alerts dan exporter/backend.
- [ ] Worked example investigasi jawaban salah, termasuk bukti yang sudah kedaluwarsa.

## 11. Security dan operasional

- [x] Bearer/ownership, scope SQL, PII guard dan pemisahan DB source/application.
- [ ] Issuer/audience/token lifecycle dashboard, tenant model dan permission PII final.
- [ ] Reauthorization pada resume, retrieval, replay dan perubahan permission.
- [ ] Otorisasi office historis, laporan tersimpan dan audit sumber Fineract; audit sumber berbeda dari audit Jarvis.
- [ ] Secrets, roles DB, data dikirim ke provider dan diagnostic-access policy.
- [ ] Deployment topology API/worker, CPU/RAM/storage dan concurrency awal.
- [ ] Numeric budgets, health/readiness, dependency outages dan graceful shutdown.
- [ ] Retention, backup/restore, recovery targets dan operational runbook.
- [ ] Maintenance readiness: compatibility/deprecation, upgrade/rollback, catalog/schema changes dan diagnosis incident.

## 12. Acceptance dan carry-over

- [x] Skenario acceptance awal ada dalam PRD/API/clarification/SSE.
- [x] Tidak mulai implementasi sebelum paket desain direview lengkap.
- [x] Penyalinan kode/dataset/migration ditunda sampai inventaris direview.
- [ ] Matriks acceptance terpusat dan expected outcome yang terukur.
- [ ] Skenario D01–D11 diturunkan menjadi acceptance terukur dengan source evidence, termasuk data tidak tersedia.
- [ ] Walkthrough sukses, klarifikasi bertingkat, hasil besar, narasi salah dan crash/reconnect.
- [ ] Inventaris aset dengan source revision, adaptasi schema dan verifikasi.
- [ ] Compatibility/golden/security/load-test strategy dan tooling final.
- [ ] Review akhir lintas dokumen dan persetujuan untuk implementasi.

## Sengaja ditunda

- Global memory: di luar rilis awal; bukan blocker selama batas ini konsisten.
- Frontend implementation: mengikuti dashboard yang ada; kontrak BE–FE tetap wajib lengkap.
- DataFusion/framework analitik tambahan: kandidat, bukan dependency wajib.
- Menyalin aset, migration dan menulis aplikasi: belum dimulai sesuai kesepakatan.

## Urutan penyelesaian berikutnya

Gap-review dataset selesai (D01–D15). Titik lanjut: susun inventaris dataset formal dari dokumen keputusan. Jangan mengulang persetujuan yang sudah tercatat. Urutan di bawah adalah dependensi penyelesaian paket teknis, bukan instruksi meninggalkan diskusi dataset atau mulai implementasi.

1. Database + transaksi + audit persistence + worker recovery, sebagai satu pembahasan terhubung.
2. Security/identity agar ownership database dan endpoint tidak dibangun atas asumsi.
3. Lengkapi schema API, clarification, response dan SSE berdasarkan lifecycle/transaksi tersebut.
4. Analytical contracts, dataset lifecycle, memory/context dan contoh end-to-end.
5. Versi stack, deployment, angka budget/retention dan acceptance matrix.
6. Review konsistensi lintas dokumen; baru putuskan readiness implementasi.
