# Jarvis — keputusan cakupan data dan titik lanjut

Tanggal: 2026-09-09. Status: keputusan kebutuhan produk disepakati; inventaris dataset formal dan kontrak teknis belum final. Dokumen ini menyimpan handoff dan lanjutan diskusi hingga persetujuan analisis kelengkapan data.

## Cara melanjutkan

- Target adalah aplikasi siap rilis penuh dan maintenance, bukan MVP. Implementasi belum diizinkan.
- Baca dokumen ini bersama [PRD](prd.md) dan [checklist](../checklist.md). PRD tetap menjadi baseline produk; dokumen ini memiliki rincian keputusan cakupan data dari diskusi ini.
- Jangan meminta ulang persetujuan cakupan di bawah. Bedakan kebutuhan yang sudah disepakati, mapping sumber yang harus diperiksa, dan keputusan bisnis yang masih terbuka.
- Gunakan repository/dokumen untuk menjawab detail yang dapat diverifikasi. Pandu diskusi satu topik setiap kali dengan contoh pertanyaan admin.
- Persetujuan kebutuhan tidak membuktikan ketersediaan data, kelengkapan riwayat, atau kesiapan kontrak eksekusi. Data contoh/demo tidak membuktikan penggunaan pada tenant aktual.
- Tidak ada kode, script, migration, dataset eksekusi, atau pilihan storage baru yang ditetapkan oleh dokumen ini.

## 1. Baseline cakupan dari handoff

| Domain | Cakupan konsep yang sudah diterima |
| --- | --- |
| Organization | Manage Offices, Manage Holidays, Manage Employees, Currency Configuration, Working Days, Payment Type, Loan Provisioning Criteria. Referensi pendukung: business date, fund terbatas, kode/nama GL untuk mapping product/provisioning, enum/reference values. Bukan persetujuan seluruh menu organization atau laporan GL. |
| Products | Loan Products, Savings Products, Share Products, Charges, Collateral Management, Delinquency Buckets, Products Mix, Fixed Deposit Products, Recurring Deposit Products, Tax Configurations, Floating Rates. Master tersedia terpisah dari account pemakai, termasuk product yang belum digunakan. |
| Client/kepemilikan | Identitas/profil bisnis, lifecycle, office/staff assignment, group/center jika digunakan, dan seluruh jenis account. Contact/address selektif; identity documents/files tidak default; notes bukan sumber kebenaran utama. |
| Loan | Identitas/relasi, lifecycle, effective terms account versus product sekarang, planned/actual disbursement bertahap, schedule dan paid/outstanding, transaksi/allocations/reversals, charges/penalties termasuk paid/waived/written-off/outstanding, balances, arrears/delinquency dengan as-of/freshness, collateral/guarantor, reschedule/perubahan terms/write-off/recovery yang tercatat. |
| Savings | Identitas/relasi, lifecycle/dormant/block, effective terms, balance versus available/holds/overdraft, deposit/withdrawal/transfer/adjustment/reversal, bunga tercatat versus posted, charges, holds, applied tax, linked accounts, riwayat tersedia. |
| FD/RD | Identitas, terms, funding, bunga, charges/tax, maturity/preclosure, transaksi dan relasi; RD juga kontribusi, schedule dan shortfall. Batas dan draft sumber ada pada bagian 2. |
| Share accounts | Identitas/lifecycle, effective terms, ownership menurut status, purchase/redemption, charges, dividends, settlement yang terbukti, history/corrections. Batas dan draft sumber ada pada bagian 2. |
| Resource penghubung | Kepemilikan account, account-product, linked accounts, transfer aktual, payment allocation, charge settlement, reversal/correction, payment type. Field dan sumber tiap hubungan belum lengkap. |

Aturan yang tetap berlaku:

- “Seluruh pinjaman Budi” berarti seluruh loan account client Budi yang telah di-resolve, tanpa default active atau perluasan ke office/group. Traversal ke seluruh office harus eksplisit dan tetap terotorisasi.
- Hubungan konfigurasi bukan bukti perpindahan dana. Jangan menyimpulkan hubungan pasti hanya dari jumlah/tanggal yang cocok.
- Transaksi, alokasi dan settlement tidak boleh menghitung uang yang sama berulang kali. Sumber savings untuk pembayaran loan hanya dinyatakan jika terbukti.
- Schedule bukan event aktual. Riwayat harus didukung sumber. Tidak ada simulasi, write action Fineract, atau valuasi tanpa sumber/kontrak.
- Master/rules, relasi, history dan performa account dipisahkan; satu join besar atas banyak child resource bukan kontrak yang benar.

## 2. Batas FD/RD dan Share yang tetap disepakati

### FD/RD

- Maturity amount tersimpan adalah proyeksi, bukan bukti payout aktual.
- Jangan mengarang rantai reinvestment tanpa FK/evidence. Instruksi closure dan bukti payout/transfer adalah hal berbeda.
- FD tidak diasumsikan memiliki installment schedule periodik. RD shortfall berbeda dari loan arrears.
- Kontrak terbuka: currency, grain/cardinality/anti-fanout, alternatif office path/alias, evidence closure payout/transfer, filter/pagination/budget/completeness, masking account number, semantik inheritance RD.

### Share

- Units berbeda dari uang; harga transaksi historis berbeda dari harga product/market sekarang.
- Pending purchase bukan ownership. Status/type/is_active menentukan measure.
- Dividen dibayar membutuhkan status posted DAN referensi savings transaction, bukan salah satunya saja.
- Linked savings tidak membuktikan settlement tiap purchase/redemption. Koreksi mengikuti evidence is_active; jangan mengasumsikan field reversal.
- Tidak mengestimasi dividen masa depan atau mark-to-market tanpa sumber/kontrak.
- Kontrak terbuka: currency/grain/anti-fanout, penyelarasan komentar paid dividend dalam draft, scope product payout versus account dividend, filter/pagination/completeness/budget, masking account number.

Draft FD/RD/share adalah referensi cakupan, bukan aset YAML siap eksekusi. Klaim verifikasi schema di dalamnya belum diverifikasi ulang terhadap deployment aktual dalam diskusi ini.

## 3. Keputusan tambahan dari lanjutan diskusi

ID D01–D15 pada dokumen ini bersifat lokal untuk keputusan dataset; berbeda dari ID ambiguitas PRD lama pada design-review.md. Rujukan lintas dokumen harus menyertakan tautan ke dokumen ini.

### D01 — Posisi historis dan laporan penutupan

Disepakati: laporan penutupan periode dapat ditampilkan kembali persis seperti saat diterbitkan, meskipun kemudian ada koreksi transaksi. Bedakan versi tersebut dari posisi historis yang dihitung ulang menggunakan informasi terbaru.

Contoh: “Tampilkan kembali posisi akhir Agustus yang diterbitkan saat penutupan, lalu bandingkan dengan posisi yang telah dikoreksi.”

Terbuka: sumber posisi historis, batas waktu penutupan, aturan penerbitan/revisi, snapshot atau mekanisme lain, retention dan reproducibility. Audit jawaban Jarvis tidak otomatis menyediakan riwayat posisi bisnis. Belum diputuskan adanya proses penutupan otomatis oleh Jarvis.

### D02 — Atribusi cabang historis

Disepakati: laporan historis menggunakan cabang pada tanggal posisi laporan. Analisis menurut cabang pengelola sekarang adalah pilihan terpisah yang diberi label jelas.

Contoh: client di Cabang A pada 31 Agustus lalu pindah ke B pada 5 September tetap masuk A untuk posisi 31 Agustus.

Terbuka: sumber riwayat assignment, snapshot, jalur office masing-masing resource dan otorisasi pembacaan historis. Atribusi historis tidak memberi akses otomatis ke data di luar izin pengguna.

### D03 — Mata uang dan konsolidasi

Disepakati: total dasar terpisah per mata uang; konsolidasi ke satu mata uang pelaporan juga termasuk kebutuhan, memakai kurs yang disetujui. Kurs yang digunakan pada laporan penutupan harus dapat ditelusuri kembali.

Contoh: “Gabungkan simpanan IDR dan USD menjadi satu total IDR per 31 Agustus.”

Terbuka: sumber kurs, arah/pasangan kurs, tanggal berlaku, jenis kurs, aturan pembulatan dan penanganan kurs tidak tersedia. Currency Configuration sendiri belum membuktikan tersedianya dataset kurs. Tidak ada provider kurs yang dipilih.

### D04 — Biaya langsung pada client

Klarifikasi kebutuhan PRD: analisis biaya lintas resource mencakup skenario biaya yang dibebankan langsung pada client. Ini menjadi pekerjaan mapping sumber, bukan persetujuan ulang kemampuan analisis.

Contoh: “Berapa biaya pendaftaran client di Cabang A, berapa yang dibayar, dan siapa yang belum lunas?”

Pisahkan master biaya dari pembebanan aktual, pembayaran, pembebasan dan outstanding. Client yang belum dibebani biaya tidak otomatis menunggak. Inventaris lama mengidentifikasi m_client_charge dan m_client_charge_paid_by; keberadaan, penggunaan, semantik dan hubungan settlement aktual perlu diverifikasi.

### D05 — Standing instructions

Disepakati: konfigurasi, status, jadwal, hasil pelaksanaan serta alasan gagal yang tercatat.

Contoh: “Instruksi pembayaran otomatis mana yang gagal bulan ini di Cabang A?”

Terbuka: mapping instruksi → percobaan pelaksanaan → transaksi aktual. Tidak adanya transfer tidak membuktikan kegagalan; instruksi bisa belum jatuh jadwal atau nonaktif. Inventaris lama menyebut standing instructions/history dan account transfer resources, bukan bukti kondisi tenant saat ini.

### D06 — Operasional kas teller/kasir

Disepakati: penugasan teller/kasir, alokasi/pengembalian kas, mutasi dan posisi kas yang didukung sumber.

Contoh: “Berapa kas tercatat per kasir di Cabang A hari ini, berikut kas masuk dan keluar?”

Terbuka: sumber/perhitungan posisi kas, hubungan transaksi dan bukti rekonsiliasi. Kas tercatat berbeda dari uang fisik; selisih kas hanya dapat dijawab jika catatan penghitungan/rekonsiliasi tersedia. Ini tidak otomatis memperluas cakupan menjadi seluruh laporan GL.

### D07 — Hasil provisioning aktual

Disepakati: hasil provisioning tersimpan, periode/tanggal proses, rincian pinjaman jika tersedia dan status pencatatannya. Ini menambah cakupan di luar provisioning criteria yang sudah diterima sebelumnya.

Contoh: “Berapa pencadangan tercatat Cabang A akhir Agustus, dan pinjaman penyumbang terbesar?”

Terbuka: sumber hasil, hubungan ke kriteria/versi yang digunakan, periode dan status. Mengalikan konfigurasi persentase sekarang dengan outstanding historis tidak membuktikan pencadangan aktual. Tidak ada simulasi provisioning atau persetujuan seluruh laporan GL.

### D08 — Kegiatan group/center

Disepakati: jadwal pertemuan, group/center, anggota, petugas, serta realisasi dan kehadiran jika didukung catatan sumber.

Contoh: “Group mana yang dijadwalkan bertemu minggu ini di Cabang A, siapa petugas dan anggotanya?”

Terbuka: sumber realisasi/kehadiran dan riwayat hubungan. Inventaris kalender tidak membuktikan pertemuan terlaksana atau anggota hadir.

### D09 — Tindakan pengguna Fineract

Disepakati: penelusuran tindakan pada resource bisnis yang disetujui, termasuk pembuatan, persetujuan, pembebasan biaya, pembatalan dan perubahan yang memiliki bukti sumber.

Contoh: “Siapa membebaskan biaya account ini, kapan, dan apa alasan yang tercatat?”

Terbuka: sumber pelaku, waktu pencatatan, alasan, nilai sebelum/sesudah, retention dan hak akses audit. Petugas pengelola bukan otomatis pelaku tindakan. Audit sumber Fineract berbeda dari audit eksekusi Jarvis; persetujuan ini tidak memberi akses ke seluruh log atau raw payload.

### D10 — Custom datatables

Disepakati: inventarisasi per deployment; hanya field dan hubungan yang telah dipetakan serta disetujui boleh dianalisis. Tabel/field aktual belum ditetapkan.

Contoh: “Client Cabang A mana yang usahanya sudah berjalan lebih dari lima tahun?”

Pemeriksaan repository Fineract menemukan contoh demo extra_client_details, extra_family_details dan extra_loan_details, dengan pemetaan melalui x_registered_table. Contoh memuat deskripsi/lama usaha dan informasi keluarga; satu client bisa mempunyai beberapa baris keluarga. Ini membuktikan contoh dukungan fitur, bukan penggunaannya oleh organisasi.

Terbuka: registry tenant aktual, struktur field, data tersedia, makna bisnis, sensitivitas, cardinality dan perubahan schema. Belum dilakukan pemeriksaan database/API deployment. Tidak perlu meminta pengguna menebak nama tabel.

### D11 — Analisis kelengkapan data

Disepakati: admin dapat meminta pemeriksaan kelengkapan dan rincian record yang perlu ditindaklanjuti, sesuai hak akses.

Contoh: “Kelompokkan outstanding pinjaman per sektor usaha di Cabang A.” Jika sektor kosong, outstanding tetap masuk total populasi yang diminta, dikelompokkan sebagai “Sektor belum tercatat”; tampilkan jumlah client/account dan outstanding yang belum terklasifikasi. Rincian record dapat diminta untuk tindak lanjut.

Data kosong tidak disamakan dengan nol atau menghilangkan record diam-diam. Aturan ini tetap menghormati filter eksplisit admin; tidak memperluas populasi yang sengaja dibatasi.

Terbuka: definisi per-field tentang kosong, tidak berlaku dan referensi tidak valid; denominator/coverage, grain hitungan client versus account, serta otorisasi rincian. Kelengkapan field bisnis berbeda dari kelengkapan eksekusi analisis dan batas preview.

### D12 — Accounting/GL: traceability, bukan laporan keuangan penuh

Disepakati: traceability journal per transaksi/account dan saldo/mutasi satu GL account bernama. Masuk: baris debit/kredit yang dihasilkan sebuah transaksi/entity (`acc_gl_journal_entry`: account_id, office_id, currency_code, transaction_id, entry_date, type_enum debit/kredit, amount) dan saldo/mutasi per `acc_gl_account` (gl_code, classification_enum, hierarchy) per office/periode.

Contoh: "Tunjukkan jurnal debit/kredit dari pembayaran pinjaman ini." atau "Saldo GL akun kas Cabang A per 31 Agustus."

Batas: bukan mesin laporan keuangan — tanpa trial balance seluruh akun, neraca, atau laba-rugi sebagai produk laporan (saldo satu akun bernama boleh; rekap seluruh ledger tidak). Jarvis tidak menghitung akuntansi sendiri (tanpa klasifikasi net income, tanpa opening/closing turunan); hanya membaca nilai tercatat dan agregasi SQL deterministik. Wajib menghormati reversal (`reversed`/`reversal_id`) dan memberi label manual versus sistem; tanpa hitung ganda. Office scope via `acc_gl_journal_entry.office_id` (GL account org-wide).

Terbuka: verifikasi kolom efektif (reversal/manual/entity/running-balance dari changeset lanjutan), interaksi closure (`acc_gl_closure`) dan koreksi D01, currency/konsolidasi D03, keandalan running balance Fineract.

### D13 — Existing Fineract reports: di luar eksekusi

Disepakati: Jarvis tetap murni analisa sendiri; TIDAK menyurfacing atau menjalankan `stretchy_report` (SQL/Pentaho buatan admin). Alasan: report bawaan membypass safety layer (office-scope, PII gate, SELECT-only single-statement, read-only, function allowlist) dan melanggar invarian "no arbitrary SQL / no unapproved surface"; juga tanpa jaminan grounding/completeness/provenance.

Contoh: "Jalankan report X Fineract." → tidak didukung; gunakan capability/analytical-contract.

Batas/opsional: boleh menyebut keberadaan report bawaan (read-only atas metadata) tanpa mengeksekusi — keputusan terpisah bila diminta. Jika sebuah report bernilai, di-port menjadi analytical-contract tervalidasi, bukan dijalankan mentah.

### D14 — Scheduler/batch job runs: terbatas plus sinyal freshness

Disepakati: riwayat eksekusi batch masuk sebagai surface operasional terbatas dan sinyal data-completeness. Masuk: definisi job (`job`: name, cron_expression) dan riwayat run (`job_run_history`: start_time, end_time, status, trigger_type). Nilai: bila COB/interest-posting/provisioning belum jalan, analisa terkait ditandai belum lengkap (nyambung D01/D07/D11).

Contoh: "Apakah interest posting Agustus sudah jalan?" atau "Job mana yang gagal semalam?"

Batas: bukan monitoring real-time; tidak men-trigger atau menjalankan ulang job (write terlarang); `error_message`/`error_log` disanitasi (tanpa stack/internal).

### D15 — Penutupan gap-review: celah sisa dari audit sistematis

Audit sistematis (220 tabel `createTable` + grouping `m_permission`) menutup tinjauan celah. Domain berat (loan origination→servicing→reschedule→collateral, savings, FD/RD, share, client/group/center, organisation, products, charges/tax, transfers/standing-instructions, teller, provisioning, GL-scoped, audit, datatables, scheduler) memetakan ke baseline atau D01–D14. Disposisi celah sisa yang disepakati:

- Surveys/PPI poverty scoring (`m_surveys`, `m_survey_responses`, `m_survey_scorecards`, `ppi_scores`, `ppi_likelihoods`): MASUK sebagai dataset/kontrak tersendiri, kondisional pemakaian deployment (pola D10).
- Credit bureau: hasil laporan (`m_creditreport` per client/loan) MASUK; config integrasi (`m_creditbureau*`) di luar.
- Loan capitalized-income/buy-down-fee balances (`m_loan_capitalized_income_balance`, `m_loan_buy_down_fee_balance`): masuk baseline Loan sebagai sub-area balance, butuh detail kontrak, bukan scope baru.
- `m_family_members`: selektif/tidak-default (pola documents/notes).
- `m_office_transaction` (kas antar-office): di bawah D06/accounting, ditandai eksplisit.

Dikecualikan (dikonfirmasi, non-analitik): documents/images, notes, campaign SMS/email, notifications, XBRL/MIX regulatory export, `m_adhoc`, webhooks/templates, external services/business events, interop, self-service/pockets/device registration, entity-to-entity mapping, auth/roles/2FA/OAuth, field-config/cache, stretchy reports (D13), laporan keuangan penuh (bagian D12 yang dikecualikan), post-dated checks.

Terbuka: verifikasi pemakaian/skema tiap domain baru pada deployment aktual; kontrak field/measure/relasi belum disusun. Persetujuan disposisi bukan finalisasi kontrak.

## 4. Referensi dan tingkat bukti

Referensi berikut berada di checkout lokal lain; bukan dependency runtime atau bukti deployment terkini:

- ai_report/docs/current/2026-09-09-jarvis-design-handoff.md — baseline kesinambungan diskusi.
- ai_report/docs/superpowers/specs/2026-09-09-fd-rd-analytical-contracts.md — draft FD/RD.
- ai_report/docs/superpowers/specs/2026-09-09-share-account-analytical-contract.md — draft Share.
- ai_report/docs/issues/active/007-analyst-grade-knowledge-and-request-mapping.md, bagian A.6 — inventaris lama client charges, standing instructions, teller/kasir dan kalender. Angka/keberadaan data dari pemeriksaan lama tidak dianggap status aktual.
- /Users/tabrezakhlaque/project/fiter/fineract/fineract-db/multi-tenant-demo-backups/default-demo/extra-datatables-and-code-values.sql, baris 72–119 — contoh tabel dan registry yang dibaca pada sesi ini; tidak dieksekusi.

Checkout ai_report: /Users/tabrezakhlaque/project/personal/rust/projects/ai_report. Tidak ada aset eksekusi yang disalin. Kontrak final harus membawa source revision dan hasil verifikasi sendiri.

## 5. Titik lanjut dan pekerjaan yang belum selesai

Tinjauan celah fungsional (gap-review) DITUTUP pada D15 setelah audit sistematis atas seluruh permukaan Fineract. Jangan menganggap daftar dataset sudah final atau mengulang persetujuan D01–D15. Langkah berikutnya adalah menyusun inventaris formal; penutupan gap-review bukan finalisasi kontrak atau bukti ketersediaan data deployment.

1. Lanjutkan tinjauan celah kebutuhan yang belum terwakili. Periksa bukti repository dahulu; tanyakan hanya keputusan bisnis yang belum terselesaikan.
2. Susun inventaris formal setelah tinjauan: kebutuhan → resource/sumber → grain → field/measure → relasi → scope → waktu/currency → evidence → acceptance.
3. Verifikasi schema dan penggunaan pada deployment, terutama history, kurs, custom datatables, realisasi pertemuan dan audit sumber.
4. Lengkapi kontrak lintas dataset: anti-fanout, filtering/sorting/pagination, completeness, budgets, masking/PII, historical authorization, versi dan maintenance schema.
5. Desain storage laporan/hasil, retention/revisi, acceptance dan operasi tetap mengikuti readiness gate. Penyimpanan catatan ini tidak mengizinkan implementasi.
