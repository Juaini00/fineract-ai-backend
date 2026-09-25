// FIN-60 (L5.1) — responses.md §1/§2 conformance, checked on documents
// actually served over HTTP.
//
// compose.rs unit tests already prove the composer CAN build conforming
// blocks. That is coverage, not conformance (build-order.md): this checks
// what GET /chat/jobs/{id}/response actually sends for real jobs, across
// every document kind the live path emits — analysis/metric,
// analysis/table, limitation, note-bearing auto-bind.

const BLOCK_TYPES = [
  "narrative",
  "metric",
  "table",
  "chart_spec",
  "comparison",
  "finding",
  "limitation",
  "suggestion",
  "note",
];

const DATA_BLOCKS = ["metric", "table", "chart_spec", "comparison", "finding"];

// §1/§2 — every block carries block_id + type (closed vocabulary) +
// schema_version, and derived_from on data-presenting blocks resolves to a
// node_run_id/dataset_id this document's evidence_json.lineage actually
// names. No `provenance`/`metrics` block ever appears (§6.2 migration).
function assertBlockShape(test, expect, document) {
  const blocks = document.blocks_json || [];
  const lineage = (document.evidence_json && document.evidence_json.lineage) || [];
  const lineageIds = new Set(
    lineage.flatMap((entry) => [entry.node_run_id, entry.dataset_id].filter(Boolean))
  );

  test("L5.1 §1–§2: setiap blok punya block_id, type ∈ kosakata, schema_version", function () {
    expect(blocks.length, "dokumen tanpa blok").to.be.above(0);
    for (const block of blocks) {
      expect(block.block_id, `block_id hilang: ${JSON.stringify(block)}`).to.be.a("string").and.not.equal("");
      expect(BLOCK_TYPES, `tipe di luar kosakata §2: ${block.type}`).to.include(block.type);
      expect(block.schema_version, `schema_version hilang: ${block.block_id}`).to.be.a("number");
      // §1/§6.2 — provenance/metrics bukan kosakata §2. Bukti negatif langsung
      // terhadap status box responses.md yang basi (2026-09-15).
      expect(block.type, `blok provenance masih terpancar: ${block.block_id}`).to.not.equal("provenance");
      expect(block.type, `blok metrics (plural) masih terpancar: ${block.block_id}`).to.not.equal("metrics");
    }
  });

  const dataBlocks = blocks.filter((block) => DATA_BLOCKS.includes(block.type));
  if (dataBlocks.length > 0) {
    test("L5.1 §2: blok penyaji data punya derived_from yang resolve ke evidence_json.lineage", function () {
      for (const block of dataBlocks) {
        expect(block.derived_from, `derived_from hilang: ${block.block_id}`).to.be.an("array").that.is.not.empty;
        for (const ref of block.derived_from) {
          const id = ref.node_run_id || ref.dataset_id;
          expect(lineageIds.has(id), `derived_from ${id} (${block.block_id}) tidak ada di evidence_json.lineage`).to.equal(true);
        }
      }
    });
  }

  test("L5.1 §1: lineage hidup di evidence_json, bukan blok `provenance`", function () {
    expect(blocks.find((block) => block.type === "provenance"), "lineage bocor jadi blok provenance").to.equal(undefined);
  });
}

module.exports = { BLOCK_TYPES, DATA_BLOCKS, assertBlockShape };
