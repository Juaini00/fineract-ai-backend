// FIN-52 (L4.0) — jawaban job dibandingkan dengan SQL langsung.
//
// `.answers/expected.js` dihasilkan `scripts/answer-expectations.sh` tepat
// sebelum tahap `answers`: baris hasil SQL langsung (ditulis terpisah dari
// capability) per capability, plus parameter yang diasumsikan SQL itu. Modul
// ini mengubah dokumen response menjadi baris yang sama bentuknya lalu
// membandingkan keduanya — bukan "angkanya ada", tetapi "angkanya benar".

const POLL_SLEEP_MS = 1000;
const POLL_DEADLINE_MS = 180000;

function expected() {
  // Dibaca saat dipakai, bukan saat modul dimuat: request yang tidak memakai
  // pembanding tidak boleh gagal hanya karena berkasnya belum dibuat.
  return require("../.answers/expected.js");
}

// Angka (termasuk NUMERIC Postgres yang dikirim sebagai string) → Number yang
// dibulatkan 6 desimal di KEDUA sisi; sisanya dibandingkan sebagai string.
function canon(value) {
  if (value === null || value === undefined) return null;
  if (typeof value === "number") return Number(value.toFixed(6));
  if (typeof value === "boolean") return value;
  const text = String(value);
  if (/^-?\d+(\.\d+)?$/.test(text)) return Number(Number(text).toFixed(6));
  return text;
}

// Jumlah office di meta.office_ids ("{1,2,...}" hasil string_agg Postgres),
// dipakai untuk menguji nilai lineage office_ids ("N authorized offices" —
// compose.rs::Bound::OfficeIds).
function officeCount(truth) {
  const matches = String(truth.meta.office_ids).match(/\d+/g);
  return matches ? matches.length : 0;
}

// Ukuran populasi penuh (tanpa row cap) untuk capability yang dipotong oleh
// hard_cap/guards.max_limit. Sumbernya berkas terpisah
// tests/answers/<capabilityId>__population.sql — satu baris `{"n": ...}` —
// dijelaskan di scripts/answer-expectations.sh.
function populationSize(capabilityId) {
  const truth = expected();
  const rows = truth.rows[capabilityId + "__population"];
  if (!Array.isArray(rows) || rows.length !== 1) {
    throw new Error(
      "tests/answers/" + capabilityId + "__population.sql belum ada atau tidak mengembalikan satu baris"
    );
  }
  return Number(rows[0].n);
}

// Response → { columns, rows: [object] }. Satu baris dijawab sebagai blok
// `metric` per kolom, banyak baris sebagai satu blok `table`, nol baris
// sebagai outcome `Empty` (compose.rs::analysis).
function answerRows(document) {
  const blocks = document.blocks_json || [];
  // Kolom PII yang ditahan selalu dinyatakan di blok `limitation`
  // `pii_withheld` (I5), apa pun bentuk jawabannya — blok `metric` tidak
  // membawa daftarnya sendiri.
  const limitation = blocks.find((b) => b.block_id === "pii_withheld");
  const withheld = (limitation && limitation.withheld_columns) || [];
  const table = blocks.find((b) => b.type === "table");
  if (table) {
    let rows;
    if (table.rows) {
      rows = table.rows.map((row) =>
        Object.fromEntries(table.columns.map((c, i) => [c, row[i]]))
      );
    } else if (table.dataset_id) {
      const dbUrl = process.env.APP_DATABASE_URL || "postgres://root:password@127.0.0.1:5432/jarvis_wt_fin_114_lc7_standing_instructions_d05_co";
      const cmd = `psql "${dbUrl}" -t -A -c "SELECT payload FROM dataset_chunks WHERE dataset_id = '${table.dataset_id}' ORDER BY chunk_index;"`;
      const out = require("child_process").execSync(cmd, { encoding: "utf8" });
      const chunks = out.trim().split("\n").filter(Boolean).map((line) => JSON.parse(line));
      rows = chunks.flat();
    } else {
      rows = [];
    }
    return {
      columns: table.columns,
      withheld: [...new Set([...withheld, ...(table.withheld_columns || [])])],
      rows,
    };
  }
  const metrics = blocks.filter((b) => b.type === "metric");
  if (metrics.length > 0) {
    const row = Object.fromEntries(metrics.map((b) => [b.key, b.value]));
    return { columns: metrics.map((b) => b.key), withheld, rows: [row] };
  }
  return { columns: [], withheld, rows: [] };
}

// Urutkan baris (array of object) menaik berdasarkan `sortBy` (daftar nama
// kolom), dibandingkan lewat `canon()`. Dipakai HANYA untuk capability yang
// production SQL-nya tidak punya total order (nit FIN-52: clients_with_
// account_counts, products_by_client, monthly_top_n dengan amount berdasi) —
// setiap kolom di `sortBy` harus membentuk kunci unik di kedua sisi, jadi
// urutan sisa (mis. `amount DESC`) tidak perlu direplikasi di sini: dua
// multiset yang sama, diurutkan sama, selalu berjajar sama persis.
function sortRows(rows, sortBy) {
  if (!sortBy || sortBy.length === 0) return rows;
  const copy = rows.slice();
  copy.sort((a, b) => {
    for (const key of sortBy) {
      const av = canon(a[key]);
      const bv = canon(b[key]);
      if (av === bv) continue;
      if (av === null) return -1;
      if (bv === null) return 1;
      return av < bv ? -1 : 1;
    }
    return 0;
  });
  return copy;
}

// Tunggu response durable: 404 selama job belum settle. Mengembalikan true
// bila response sudah ada; false berarti request ini dijadwalkan ulang. Nama
// request untuk polling diambil dari `req.getName()` (bru 4.0.0,
// @usebruno/js bruno-request.js) — bukan string literal yang diketik ulang —
// supaya nama poll dan nama request TIDAK PERNAH bisa berbeda (lihat bug
// FIN-52: account-identity-lookup-answer.yml pernah memakai nama tanpa
// suffix " (TIDAK TERJANGKAU)" sehingga bru.setNextRequest gagal diam-diam).
//
// Batasnya WAKTU, bukan jumlah poll: `bru.setNextRequest` mengulang request
// tanpa jeda `--delay`, sehingga 60 poll habis dalam ±2 detik sementara resume
// sesudah klarifikasi bisa menunggu job lain di worker tunggal (±8 detik).
// Tiap poll tidur POLL_SLEEP_MS; menyerah sesudah POLL_DEADLINE_MS.
async function awaitResponse(bru, req, res) {
  if (res.getStatus() !== 404) return true;
  const requestName = req.getName();
  const key = "answersPollStart_" + requestName.replace(/[^A-Za-z0-9_.-]/g, "_");
  const started = Number(bru.getVar(key) || 0) || Date.now();
  bru.setVar(key, started);
  if (Date.now() - started > POLL_DEADLINE_MS) {
    throw new Error(
      `${requestName}: response tidak tersedia dalam ${POLL_DEADLINE_MS / 1000} detik`
    );
  }
  await new Promise((resolve) => setTimeout(resolve, POLL_SLEEP_MS));
  bru.setNextRequest(requestName);
  return false;
}

// office_ids selalu diungkapkan sebagai "N authorized offices"
// (compose.rs::Bound::OfficeIds) — N harus sama dengan jumlah office di
// meta.office_ids yang dipakai `scripts/answer-expectations.sh` untuk
// menghitung SQL pembanding, supaya scope job dan scope SQL pembanding
// dibuktikan sama, bukan cuma diasumsikan.
function expectOfficeIds(test, expect, capabilityId, document, truth, narrowedTo) {
  // FIN-115: `narrowedTo` (a count) proves a job whose body narrowed office_ids
  // below the caller's authorization; its oracle hard-codes the same offices.
  const wanted = narrowedTo === undefined ? officeCount(truth) : narrowedTo;
  const label = narrowedTo === undefined ? "all authorized offices" : `narrowed to ${narrowedTo} offices`;
  test(`FIN-52 ${capabilityId}: office_ids = ${label}`, function () {
    const param = document.evidence_json.lineage[0].parameters.find((p) => p.name === "office_ids");
    expect(param, "parameter office_ids tidak ada di lineage").to.exist;
    expect(param.value).to.equal(`${wanted} authorized offices`);
  });
}

function expectScope(test, expect, capabilityId, document, truth, narrowedTo) {
  // FIN-124 / D14 (owner decision): operational scheduler data is tenant-wide for
  // an admin, declared separately from org_wide_reference — no office parameter
  // is bound and the scope/meaning disclosure travels with the answer.
  if (document.evidence_json.lineage[0].scope_class === "operational_tenant_wide") {
    test(`FIN-124 ${capabilityId}: operational tenant-wide scope is disclosed, no office filter invented`, function () {
      const lineage = document.evidence_json.lineage[0];
      expect(lineage.parameters.some((p) => p.name === "office_ids")).to.equal(false);
      expect(document.blocks_json.some((b) => b.block_id === "org_wide_reference")).to.equal(false);
      const scope = document.blocks_json.find((b) => b.block_id === "operational_scope");
      expect(scope, "blok operational_scope").to.exist;
      expect(scope.type).to.equal("limitation");
      expect(scope.scope_class).to.equal("operational_tenant_wide");
    });
    return;
  }
  if (document.evidence_json.lineage[0].scope_class === "org_wide_reference") {
    test(`FIN-109 ${capabilityId}: tenant-wide master is disclosed, not office usage`, function () {
      expect(document.evidence_json.lineage[0].parameters.some((p) => p.name === "office_ids")).to.equal(false);
      expect(document.blocks_json.some((b) => b.block_id === "org_wide_reference" && b.type === "note")).to.equal(true);
    });
    return;
  }
  expectOfficeIds(test, expect, capabilityId, document, truth, narrowedTo);
}

// Kolom jawaban job (dikurangi yang ditahan) harus SAMA PERSIS dengan kolom
// baris acuan — bukan cuma superset/subset. `columns` adalah daftar nama
// kolom SQL pembanding (biasanya `Object.keys` baris pertama).
function expectColumnSet(test, expect, capabilityId, document, columns) {
  test(`FIN-52 ${capabilityId}: job answer columns = direct SQL columns`, function () {
    const got = answerRows(document);
    const keys = columns.filter((k) => !got.withheld.includes(k));
    for (const column of got.columns) {
      expect(keys, `kolom ${column} tidak ada di SQL langsung`).to.include(column);
    }
    for (const key of keys) {
      expect(got.columns, `kolom ${key} tidak ada di jawaban job`).to.include(key);
    }
  });
}

// Satu pemeriksaan penuh untuk satu capability. `test()` milik Bruno dioper
// masuk karena ia hanya ada di scope skrip request. `options.sortBy`:
// capability yang production SQL-nya tidak punya total order (lihat
// `sortRows`) — kedua sisi diurutkan sebelum dibandingkan baris demi baris,
// supaya urutan hasil planner (plan luck) tidak membuat gate ini flaky.
function check(test, expect, capabilityId, document, options) {
  const truth = expected();
  // FIN-115: `truthKey` names a second oracle for the same capability (e.g. a
  // job narrowed to fewer offices); `narrowedTo` is that job's office count.
  const truthKey = (options && options.truthKey) || capabilityId;
  const want = truth.rows[truthKey];
  const assumed = truth.params[truthKey];
  const sortBy = options && options.sortBy;

  test(`FIN-52 ${capabilityId}: answered by the right capability`, function () {
    expect(want, "tests/answers/" + truthKey + ".sql belum ada").to.be.an("array");
    expect(document.kind).to.equal("analysis");
    expect(["Answered", "Empty"]).to.include(document.outcome);
    expect(document.evidence_json.lineage[0].capability_id).to.equal(capabilityId);
  });

  test(`FIN-52 ${capabilityId}: job parameters = direct SQL parameters`, function () {
    const bound = {};
    for (const p of document.evidence_json.lineage[0].parameters) {
      if (p.name !== "office_ids") bound[p.name] = p.value;
    }
    expect(bound).to.deep.equal(assumed);
  });

  expectScope(test, expect, capabilityId, document, truth, options && options.narrowedTo);
  expectColumnSet(test, expect, capabilityId, document, (want || []).length > 0 ? Object.keys(want[0]) : []);

  test(`FIN-52 ${capabilityId}: job answer = direct SQL`, function () {
    const got = answerRows(document);
    expect(got.rows.length, "jumlah baris").to.equal(want.length);
    const wantRows = sortRows(want, sortBy);
    const gotRows = sortRows(got.rows, sortBy);
    for (let i = 0; i < wantRows.length; i++) {
      const keys = Object.keys(wantRows[i]).filter((k) => !got.withheld.includes(k));
      for (const key of keys) {
        expect(canon(gotRows[i][key]), `baris ${i}, ${key}`).to.equal(canon(wantRows[i][key]));
      }
    }
  });
}

// Capability yang sengaja nondeterministik (mis. sampel acak): baris jawaban
// tidak bisa diadu urut. SQL langsung mengembalikan SELURUH populasi yang sah;
// setiap baris jawaban wajib ada di sana apa adanya, tanpa duplikat, dan
// jumlahnya sama dengan `min(populasi, cap)` — `cap` adalah batas yang
// diikat job (mis. defaults.default_limit); jumlah populasi TIDAK di-hard-
// code di berkas answer, supaya penambahan/penghapusan data tidak diam-diam
// membuat test ini vacuous atau merah untuk alasan yang salah.
function checkSubset(test, expect, capabilityId, document, cap) {
  const truth = expected();
  const population = truth.rows[capabilityId];
  const expectedCount = population ? Math.min(population.length, cap) : cap;

  test(`FIN-52 ${capabilityId}: answered by the right capability`, function () {
    expect(population, "tests/answers/" + capabilityId + ".sql belum ada").to.be.an("array");
    expect(document.kind).to.equal("analysis");
    expect(["Answered", "Empty"]).to.include(document.outcome);
    expect(document.evidence_json.lineage[0].capability_id).to.equal(capabilityId);
  });

  test(`FIN-52 ${capabilityId}: job parameters = direct SQL parameters`, function () {
    const bound = {};
    for (const p of document.evidence_json.lineage[0].parameters) {
      if (p.name !== "office_ids") bound[p.name] = p.value;
    }
    expect(bound).to.deep.equal(truth.params[capabilityId]);
  });

  expectScope(test, expect, capabilityId, document, truth);
  expectColumnSet(test, expect, capabilityId, document, (population || []).length > 0 ? Object.keys(population[0]) : []);

  test(`FIN-52 ${capabilityId}: every answer row is in the direct SQL population`, function () {
    const got = answerRows(document);
    expect(got.rows.length).to.equal(expectedCount);
    const key = (row) => JSON.stringify(got.columns.map((c) => canon(row[c])));
    const allowed = new Set(population.map(key));
    const seen = new Set();
    for (const row of got.rows) {
      expect(allowed.has(key(row)), key(row)).to.equal(true);
      expect(seen.has(key(row)), "duplikat " + key(row)).to.equal(false);
      seen.add(key(row));
    }
  });
}

// FIN-133 — semantik row cap: capability dengan `limit.default: unbounded`
// dan `hard_cap`/`guards.max_limit` terdeklarasi memotong ke `cap` baris,
// bukan mengembalikan seluruh riwayat. Bila populasi sebenarnya (dihitung
// terpisah, lihat `populationSize`) melebihi `cap`: completeness harus
// Partial dengan alasan row_cap_reached, dan response membawa blok
// limitation `row_cap_reached` (row_cap/rows_shown = cap, more_rows_exist =
// true, tanpa derived_from). Bila populasi <= cap: tidak ada pemotongan,
// jadi tidak boleh ada blok itu dan completeness harus Complete.
function expectRowCap(test, expect, capabilityId, document, cap, populationSize) {
  test(`FIN-52 ${capabilityId}: row cap ${cap} — completeness follows the population size (${populationSize})`, function () {
    const limitation = (document.blocks_json || []).find((b) => b.block_id === "row_cap_reached");
    if (populationSize > cap) {
      expect(document.completeness, "completeness").to.equal("Partial");
      expect(document.completeness_reason, "completeness_reason").to.equal("row_cap_reached");
      expect(limitation, "blok limitation row_cap_reached tidak ada").to.exist;
      expect(limitation.type).to.equal("limitation");
      expect(limitation.derived_from).to.equal(undefined);
      expect(limitation.row_cap).to.equal(cap);
      expect(limitation.rows_shown).to.equal(cap);
      expect(limitation.more_rows_exist).to.equal(true);
      expect(limitation.title, "title").to.be.a("string").and.not.equal("");
      expect(limitation.body, "body").to.be.a("string").and.not.equal("");
    } else {
      expect(document.completeness, "completeness").to.equal("Complete");
      expect(limitation, "blok limitation row_cap_reached tidak boleh ada saat populasi <= cap").to.equal(undefined);
    }
  });
}

module.exports = {
  answerRows,
  awaitResponse,
  canon,
  check,
  checkSubset,
  expectRowCap,
  populationSize,
};
