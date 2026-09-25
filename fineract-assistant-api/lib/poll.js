// Polling snapshot job untuk tahap recovery (FIN-141). Batasnya WAKTU, bukan
// jumlah poll (lihat answers.js awaitResponse): `bru.setNextRequest` mengulang
// request tanpa jeda `--delay`, jadi tiap poll tidur POLL_SLEEP_MS sendiri.
const POLL_SLEEP_MS = 1000;
const POLL_DEADLINE_MS = 90000;

const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

// true bila `done(job)` terpenuhi; false bila request ini dijadwalkan ulang.
async function awaitJob(bru, req, res, done) {
  if (res.getStatus() === 200 && done(res.getBody().data)) return true;
  const requestName = req.getName();
  const key = "jobPollStart_" + requestName.replace(/[^A-Za-z0-9_.-]/g, "_");
  const started = Number(bru.getVar(key) || 0) || Date.now();
  bru.setVar(key, started);
  if (Date.now() - started > POLL_DEADLINE_MS) {
    throw new Error(`${requestName}: kondisi job tidak tercapai dalam ${POLL_DEADLINE_MS / 1000} detik`);
  }
  await sleep(POLL_SLEEP_MS);
  bru.setNextRequest(requestName);
  return false;
}

function replay(res) {
  return String(res.getBody())
    .split("\n")
    .filter((line) => line.startsWith("data: "))
    .map((line) => JSON.parse(line.slice(6)));
}

const TERMINAL = ["Completed", "Failed", "Cancelled", "Expired"];

module.exports = { awaitJob, replay, sleep, TERMINAL };
