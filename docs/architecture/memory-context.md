# Memori session dan konteks LLM

**Status: kontrak desain.** Dokumen ini merinci keputusan memori/konteks yang sudah disepakati; bukan otorisasi implementasi.

**Sumber kebenaran:** [database-design.md](../data/database-design.md) §1, §2.1 (C12/C13), §4.5, §4.7, §5, §6 (T7/T8) adalah kontrak schema dan transaksi. Bila dokumen ini bertentangan dengannya, `database-design.md` menang. Lifecycle/titik promosi dimiliki [engine.md](engine.md); batas produk dimiliki [prd.md](../product/prd.md) §8; budget per panggilan dimiliki [runtime.md](../operations/runtime.md) §6. [responses.md](../contracts/responses.md) hanya menjadi konteks untuk response document.

---

## 1. Empat lapisan yang dipisahkan

| Lapisan | Isi | Penyimpanan | Batas penggunaan |
| --- | --- | --- | --- |
| Job execution state | lifecycle job, lease, plan/node attempt, event, response dan dataset | Durable di PostgreSQL (`chat_jobs`, `job_plans`, `job_node_runs`, `job_events`, `job_responses`, `datasets`) | Dipakai Engine untuk menjalankan atau memulihkan job; bukan percakapan LLM penuh. |
| Durable session history | Jejak turn user, assistant dan clarification | Durable di PostgreSQL (`chat_messages`) | Indeksnya tipis; riwayat besar dipaginasi, tidak dimuat seluruhnya per request. |
| Structured session facts | Fakta follow-up yang relevan, berprovenance dan berversi status | Durable di PostgreSQL (`session_memory`) | Hanya referensi/fakta terstruktur yang telah dipromosikan dari response durable. |
| Per-call LLM working set | Instruksi, contract/schema, turn terpilih, binding, evidence, memori relevan dan ruang output/reasoning | Hanya working set sementara | Dirakit ulang sebelum **setiap** model call dan dibuang setelah call; bukan source of truth. |

Redis hanya koordinasi live, bukan pengganti salah satu lapisan durable di atas (PRD §8). `context_json` lama tidak ada pada `chat_sessions`; ringkasan memori bukan substitusi untuk history maupun facts terstruktur.

## 2. `session_memory`: fakta session terstruktur

Satu baris `session_memory` selalu terikat pada `session_id`, `owner_user_id`, `session_seq`, provenance, completeness, dan response sumber. `session_seq` meningkat tanpa celah dengan row lock pada `chat_sessions.memory_seq_last` (I3).

| `kind` | Isi dan kapan menjadi kandidat promosi | Keunikan fakta valid |
| --- | --- | --- |
| `PriorResult` | Referensi hasil/temuan sebelumnya yang dapat dipakai untuk follow-up, termasuk provenance, completeness, dan bila ada `dataset_id`. Ditulis hanya saat response yang memuat hasil itu di-commit. | Tidak ada constraint unik tambahan yang menghapus riwayat hasil; relevansi dan status menentukan pemakaian. |
| `ResolvedEntity` | Identitas yang telah terselesaikan untuk `entity_key` (misalnya hasil resolver yang dipakai response). Ditulis saat response commit membuktikan resolusinya durable. | Paling banyak satu baris `valid` per `(session_id, entity_key)`. `entity_key` wajib ada. |
| `ActiveScope` | Scope aktif yang telah dipakai response—misalnya filter/scope yang diperlukan follow-up—tanpa `dataset_id` maupun `entity_key`. | Paling banyak satu `ActiveScope` `valid` per session. |

Promosi tidak melakukan hard delete saat fakta lama tidak lagi berlaku. Baris lama diberi status `superseded` dan menunjuk `superseded_by_id`; invalidasi/eviction juga tetap meninggalkan jejak status, alasan, dan waktu. Dengan demikian provenance, urutan keputusan, dan alasan fakta lama tidak lagi dipakai tetap dapat diaudit. Hard delete hanya mengikuti penghapusan session melalui `session_memory.session_id → chat_sessions` `CASCADE`; FK ke response sumber adalah `NO ACTION`, bukan `RESTRICT` (database-design.md §5, I2).

## 3. Titik promosi tunggal

**Satu-satunya titik promosi adalah response commit T7**, termasuk T8 skip. Dalam transaksi lokal yang sama, Engine menyimpan `job_responses`, menyelesaikan `chat_jobs`, mengalokasikan `memory_seq_last`, memasukkan `session_memory`, menulis `chat_messages(role='assistant')`, event terminal, dan audit. Tidak ada promosi inkremental dari node, resolver, atau checkpoint.

Konsekuensinya:

- C12/K4 mewajibkan `source_response_version NOT NULL` dan FK komposit ke `(source_job_id, source_response_version)` pada `job_responses`; fakta memori tidak mungkin ada tanpa response durable.
- Crash sebelum commit berarti tidak ada baris memori baru. Output node/checkpoint yang sudah durable hanya boleh dipakai untuk pemulihan job yang sama.
- Bila job tidak pernah selesai, pertanyaan berikutnya menggunakan retrieval baru, bukan meneruskan context secara diam-diam.
- Skip tetap menghasilkan response `kind='skipped'`, gap eksplisit, dan promosi atomik. Cancel tidak memiliki kewajiban response maupun memory promotion (engine.md, bagian *Skip sebagai jalur terminal*).

## 4. Ringkasan memori inkremental

`chat_sessions` menyimpan `memory_summary_text`, `memory_summary_version`, `memory_summary_watermark`, `memory_summary_status`, `memory_summary_updated_at`, dan `memory_seq_last`.

| Field/status | Kontrak |
| --- | --- |
| `memory_summary_watermark` | `session_seq` tertinggi yang sudah tercakup dalam ringkasan. Ia memungkinkan pembaruan inkremental atas fakta yang dipromosikan setelah watermark. |
| `memory_summary_version` | Versi ringkasan, terpisah dari `session_seq` dan response version. |
| `current` | Ringkasan tersedia dan cocok dengan watermark-nya. |
| `stale` | Ringkasan perlu diperbarui; khususnya bila penghitungan setelah T7/T8 gagal. Watermark **tidak bergerak** pada kegagalan itu. |
| `failed` | Status kegagalan yang disimpan schema; tidak boleh diperlakukan sebagai ringkasan yang otoritatif. |

Ringkasan dihitung **setelah** T7/T8 commit, di luar transaksi, karena memerlukan LLM (I1). Ia hanya memproses delta setelah watermark yang relevan dan tidak boleh menahan transaksi PostgreSQL selama external call. Kegagalan penghitungan menandai `memory_summary_status='stale'`; response dan memory facts hasil commit tetap durable.

Ringkasan membantu seleksi konteks, tetapi **tidak pernah** menjadi dasar otorisasi atau sumber angka/numerik (I7). Angka, completeness, dan evidence tetap berasal dari ledger/response/dataset durable serta validator response.

## 5. Seleksi konteks dan budget token

Sebelum setiap model call, Engine menghitung budget gabungan untuk instruksi, schema/contract, history terpilih, session memory yang relevan, binding, evidence, output/reasoning reserve, dan safety margin (PRD §6; runtime.md §6). Pemilihan konteks bersifat terarah terhadap intent/capability saat ini; bukan pemuatan seluruh history atau seluruh `session_memory`.

Bila gabungan melampaui budget, urutannya adalah:

1. buang context/preview opsional;
2. gunakan agregasi semantik yang disetujui atau ringkasan yang masih sesuai watermark;
3. lakukan retrieval terarah yang tetap berada dalam budget;
4. kembalikan hasil berkualifikasi atau limitation operasional bila masih tidak muat.

Overflow **tidak pernah** membatasi dates, office, atau population secara diam-diam. Pengurangan scope semacam itu adalah perubahan makna permintaan dan wajib dijelaskan/ditangani melalui jalur kontrak yang sesuai, bukan optimasi context.

Handle dataset yang expired/purged selalu eksplisit melalui C13 (`handle_state` non-optional pada tipe hasil repository). Ia tidak boleh dipakai sebagai continuation tersembunyi.

## 6. Konkurensi, freshness, dan otorisasi

- **Satu job nonterminal per session.** Partial unique `chat_jobs(session_id)` untuk lifecycle nonterminal mencegah dua job aktif—termasuk `WaitingForUser`—mempromosikan fakta yang bersaing dalam satu session (C2).
- **Tidak menimpa state lebih baru.** Saat melakukan promosi/supersede, transaksi wajib menggunakan state valid saat ini dan urutan `session_seq`; promosi yang stale tidak boleh mengganti `ActiveScope` atau `ResolvedEntity` valid yang lebih baru. Riwayat yang telah ada dipertahankan sebagai superseded, bukan ditulis ulang.
- **Owner difilter setiap baca.** Pembacaan session, history, summary, dan `session_memory` selalu membatasi `owner_user_id`; fakta session bukan capability/grant.
- **Dataset diotorisasi ulang setiap baca.** Reference dataset dalam `PriorResult` tidak mengesahkan akses ulang. Repository membawa `handle_state` dan otorisasi/scope diperiksa kembali pada setiap pembacaan (I7, C13). `scope_json` yang tersimpan adalah bukti konteks keputusan/audit, bukan grant yang dapat digunakan ulang.

## 7. Skenario acceptance

1. `MEM-7.1` — **Response normal:** T7 commit menyimpan response dan facts dalam satu transaksi; setiap `session_memory` baru memiliki `source_response_version` yang menunjuk response tersebut.
2. `MEM-7.2` — **Crash pra-commit:** worker berhenti sebelum T7 selesai; tidak ada facts baru. Jika job tidak dipulihkan sampai terminal, follow-up melakukan retrieval baru.
3. `MEM-7.3` — **Skip:** T8 menghasilkan response `skipped`, gap eksplisit, terminal event, dan promosi memori atomik; cancel tidak mempromosikan memori.
4. `MEM-7.4` — **Summary gagal:** facts/response hasil commit tetap ada; summary berstatus `stale`, watermark lama tetap utuh, dan call berikutnya tidak menganggap ringkasan itu current.
5. `MEM-7.5` — **Follow-up overflow:** Engine mengurangi preview/context opsional atau memakai agregasi/retrieval terarah; ia tidak mengubah periode, office, atau population tanpa pengungkapan.
6. `MEM-7.6` — **Entity/scope baru:** fakta valid lama ditandai superseded, bukan dihapus; constraint menolak dua `ResolvedEntity` valid untuk `entity_key` sama atau dua `ActiveScope` valid dalam satu session.
7. `MEM-7.7` — **Akses ulang:** user yang bukan owner tidak dapat membaca memory/session; dataset handle dari prior result tetap gagal dipakai bila re-authorize atau `handle_state` tidak mengizinkan.
