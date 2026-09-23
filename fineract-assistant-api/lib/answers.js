// FIN-52 (L4.0) — jawaban job dibandingkan dengan SQL langsung.
//
// `.answers/expected.js` dihasilkan `scripts/answer-expectations.sh` tepat
// sebelum tahap `answers`: baris hasil SQL langsung (ditulis terpisah dari
// capability) per capability, plus parameter yang diasumsikan SQL itu. Modul
// ini mengubah dokumen response menjadi baris yang sama bentuknya lalu
// membandingkan keduanya — bukan "angkanya ada", tetapi "angkanya benar".

const MAX_POLLS = 60;

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
    return {
      columns: table.columns,
      withheld: [...new Set([...withheld, ...(table.withheld_columns || [])])],
      rows: table.rows.map((row) =>
        Object.fromEntries(table.columns.map((c, i) => [c, row[i]]))
      ),
    };
  }
  const metrics = blocks.filter((b) => b.type === "metric");
  if (metrics.length > 0) {
    const row = Object.fromEntries(metrics.map((b) => [b.key, b.value]));
    return { columns: metrics.map((b) => b.key), withheld, rows: [row] };
  }
  return { columns: [], withheld, rows: [] };
}

// Tunggu response durable: 404 selama job belum settle. Mengembalikan true bila
// response sudah ada; false berarti request ini dijadwalkan ulang.
function awaitResponse(bru, res, requestName) {
  if (res.getStatus() !== 404) return true;
  const key = "answersPolls_" + requestName.replace(/[^A-Za-z0-9_.-]/g, "_");
  const polls = Number(bru.getVar(key) || 0);
  if (polls >= MAX_POLLS) {
    throw new Error(requestName + ": response tidak pernah tersedia");
  }
  bru.setVar(key, polls + 1);
  bru.setNextRequest(requestName);
  return false;
}

// Satu pemeriksaan penuh untuk satu capability. `test()` milik Bruno dioper
// masuk karena ia hanya ada di scope skrip request.
function check(test, expect, capabilityId, document) {
  const truth = expected();
  const want = truth.rows[capabilityId];
  const assumed = truth.params[capabilityId];

  test(`FIN-52 ${capabilityId}: dijawab oleh capability yang benar`, function () {
    expect(want, "tests/answers/" + capabilityId + ".sql belum ada").to.be.an("array");
    expect(document.kind).to.equal("analysis");
    expect(["Answered", "Empty"]).to.include(document.outcome);
    expect(document.evidence_json.lineage[0].capability_id).to.equal(capabilityId);
  });

  test(`FIN-52 ${capabilityId}: parameter job = parameter SQL langsung`, function () {
    const bound = {};
    for (const p of document.evidence_json.lineage[0].parameters) {
      if (p.name !== "office_ids") bound[p.name] = p.value;
    }
    expect(bound).to.deep.equal(assumed);
  });

  test(`FIN-52 ${capabilityId}: jawaban job = SQL langsung`, function () {
    const got = answerRows(document);
    expect(got.rows.length, "jumlah baris").to.equal(want.length);
    for (let i = 0; i < want.length; i++) {
      const keys = Object.keys(want[i]).filter((k) => !got.withheld.includes(k));
      for (const column of got.columns) {
        expect(keys, `kolom ${column} tidak ada di SQL langsung`).to.include(column);
      }
      for (const key of keys) {
        expect(got.columns, `kolom ${key} tidak ada di jawaban job`).to.include(key);
        expect(canon(got.rows[i][key]), `baris ${i}, ${key}`).to.equal(canon(want[i][key]));
      }
    }
  });
}

// Capability yang sengaja nondeterministik (mis. sampel acak): baris jawaban
// tidak bisa diadu urut. SQL langsung mengembalikan SELURUH populasi yang sah;
// setiap baris jawaban wajib ada di sana apa adanya, tanpa duplikat, dan
// jumlahnya sama dengan `expectedCount`.
function checkSubset(test, expect, capabilityId, document, expectedCount) {
  const truth = expected();
  const population = truth.rows[capabilityId];

  test(`FIN-52 ${capabilityId}: dijawab oleh capability yang benar`, function () {
    expect(population, "tests/answers/" + capabilityId + ".sql belum ada").to.be.an("array");
    expect(document.evidence_json.lineage[0].capability_id).to.equal(capabilityId);
  });

  test(`FIN-52 ${capabilityId}: parameter job = parameter SQL langsung`, function () {
    const bound = {};
    for (const p of document.evidence_json.lineage[0].parameters) {
      if (p.name !== "office_ids") bound[p.name] = p.value;
    }
    expect(bound).to.deep.equal(truth.params[capabilityId]);
  });

  test(`FIN-52 ${capabilityId}: setiap baris jawaban ada di populasi SQL langsung`, function () {
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

module.exports = { answerRows, awaitResponse, canon, check, checkSubset };
