/**
 * SFTP UX helpers (#43).
 */

import {
  SFTP_ROW_COMPACT_MIN_PX,
  batchDownloadEnabled,
  batchUploadEnabled,
  filterFileEntries,
  isHiddenEntry,
  shouldShowBatchBar,
  sftpSearchPlaceholder,
} from "./sftpUx.js";

function check(name, ok, detail) {
  return { name, ok, detail };
}

function runTests() {
  const checks = [];
  const entries = [
    { name: "docs" },
    { name: ".cache" },
    { name: "notes.txt" },
    { name: ".." },
  ];

  checks.push(check("isHiddenEntry", isHiddenEntry(".cache") && !isHiddenEntry("docs") && !isHiddenEntry(".."), ""));

  const hiddenOff = filterFileEntries(entries, { query: "", showHidden: false });
  checks.push(
    check(
      "filter hides dotfiles by default",
      hiddenOff.length === 3 && hiddenOff.every((e) => !isHiddenEntry(e.name)),
      hiddenOff.map((e) => e.name).join(","),
    ),
  );

  const withHidden = filterFileEntries(entries, { query: "", showHidden: true });
  checks.push(check("filter showHidden", withHidden.length === 4, String(withHidden.length)));

  const q = filterFileEntries(entries, { query: "note", showHidden: false });
  checks.push(check("filter query", q.length === 1 && q[0].name === "notes.txt", q[0]?.name));

  checks.push(
    check(
      "shouldShowBatchBar with selection",
      shouldShowBatchBar({ localSelected: 1, remoteSelected: 0 }),
      "",
    ),
  );
  checks.push(
    check(
      "shouldShowBatchBar empty selection",
      !shouldShowBatchBar({ localSelected: 0, remoteSelected: 0 }),
      "",
    ),
  );
  checks.push(
    check(
      "shouldShowBatchBar single-pane remote selection",
      shouldShowBatchBar({ localSelected: 0, remoteSelected: 2 }),
      "",
    ),
  );

  checks.push(check("batchUploadEnabled", batchUploadEnabled(2) && !batchUploadEnabled(0), ""));
  checks.push(check("batchDownloadEnabled", batchDownloadEnabled(1) && !batchDownloadEnabled(0), ""));
  checks.push(
    check("sftpSearchPlaceholder", sftpSearchPlaceholder("pane A") === "Filter pane A…", sftpSearchPlaceholder("pane A")),
  );
  checks.push(check("SFTP_ROW_COMPACT_MIN_PX", SFTP_ROW_COMPACT_MIN_PX === 28, String(SFTP_ROW_COMPACT_MIN_PX)));

  return checks;
}

const results = runTests();
let failed = 0;
for (const r of results) {
  const mark = r.ok ? "ok" : "FAIL";
  if (!r.ok) failed += 1;
  console.log(`${mark}  ${r.name}${r.detail ? ` — ${r.detail}` : ""}`);
}
console.log(`\n${results.length - failed}/${results.length} passed`);
process.exit(failed ? 1 : 0);
