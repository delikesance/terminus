/**
 * Red/Green tests for SFTP free Host A/B layout (#41).
 */

import {
  SFTP_LOCAL_ID,
  canTransferBetween,
  createSftpBrowserState,
  defaultSplitCompanion,
  enterSplit,
  exitSplit,
  focusRemoteInLayout,
  isLocalEndpoint,
  openSingle,
  placeAdditionalRemote,
  setPaneEndpoint,
  shouldShowTransferUi,
  transferDirection,
} from "./sftpLayout.js";

function check(name, ok, detail) {
  return { name, ok, detail };
}

function runTests() {
  const checks = [];

  {
    const s = createSftpBrowserState();
    checks.push(
      check(
        "AC1 default single with local paneA",
        s.mode === "single" && s.paneA === SFTP_LOCAL_ID && s.paneB === null,
        JSON.stringify(s),
      ),
    );
  }

  {
    const s = openSingle(createSftpBrowserState(), "host-1");
    checks.push(
      check(
        "AC1 openSingle remote",
        s.mode === "single" && s.paneA === "host-1" && s.paneB === null && !shouldShowTransferUi(s),
        JSON.stringify(s),
      ),
    );
  }

  {
    checks.push(check("AC2 isLocalEndpoint", isLocalEndpoint(SFTP_LOCAL_ID) && !isLocalEndpoint("h1"), ""));
  }

  {
    let s = openSingle(createSftpBrowserState(), "host-1");
    s = enterSplit(s, ["host-1", "host-2"]);
    checks.push(
      check(
        "AC3 enterSplit pairs remote with local",
        s.mode === "split" &&
          s.paneA === "host-1" &&
          s.paneB === SFTP_LOCAL_ID &&
          shouldShowTransferUi(s),
        JSON.stringify(s),
      ),
    );
  }

  {
    const companion = defaultSplitCompanion(SFTP_LOCAL_ID, ["h1", "h2"]);
    checks.push(check("defaultSplitCompanion from local picks first host", companion === "h1", companion));
  }

  {
    let s = enterSplit(openSingle(createSftpBrowserState(), "h1"), ["h1", "h2"]);
    s = setPaneEndpoint(s, "b", "h2");
    checks.push(
      check(
        "AC4 free Host B remote",
        s.paneA === "h1" && s.paneB === "h2" && !canTransferBetween(s.paneA, s.paneB),
        JSON.stringify(s),
      ),
    );
    const bothRemote = transferDirection(s.paneA, s.paneB);
    s = setPaneEndpoint(s, "b", SFTP_LOCAL_ID);
    const dirOk = transferDirection(s.paneA, s.paneB);
    checks.push(
      check(
        "AC4 transfer only local↔remote",
        bothRemote === null &&
          dirOk != null &&
          dirOk.localSlot === "b" &&
          dirOk.remoteSlot === "a" &&
          dirOk.remoteHostId === "h1",
        JSON.stringify({ bothRemote, dirOk }),
      ),
    );
  }

  {
    let s = enterSplit(openSingle(createSftpBrowserState(), "h1"), ["h1"]);
    s = exitSplit(s);
    checks.push(
      check(
        "AC5 exitSplit keeps paneA single",
        s.mode === "single" && s.paneA === "h1" && s.paneB === null,
        JSON.stringify(s),
      ),
    );
  }

  // #103 — opening a second remote must keep the first visible
  {
    const s = placeAdditionalRemote(openSingle(createSftpBrowserState(), "host-a"), "host-b");
    checks.push(
      check(
        "AC103 single remote + open B → split A|B (A stays)",
        s.mode === "split" && s.paneA === "host-a" && s.paneB === "host-b",
        JSON.stringify(s),
      ),
    );
  }

  {
    let s = enterSplit(openSingle(createSftpBrowserState(), "host-a"), ["host-a"]);
    s = placeAdditionalRemote(s, "host-b");
    checks.push(
      check(
        "AC103 split A|local + open B → A|B (local replaced, A stays)",
        s.mode === "split" && s.paneA === "host-a" && s.paneB === "host-b",
        JSON.stringify(s),
      ),
    );
  }

  {
    let s = { mode: "split", paneA: SFTP_LOCAL_ID, paneB: "host-a" };
    s = placeAdditionalRemote(s, "host-b");
    checks.push(
      check(
        "AC103 split local|A + open B → B|A (local replaced)",
        s.mode === "split" && s.paneA === "host-b" && s.paneB === "host-a",
        JSON.stringify(s),
      ),
    );
  }

  {
    const s0 = { mode: "split", paneA: "host-a", paneB: "host-b" };
    const s = placeAdditionalRemote(s0, "host-b");
    checks.push(
      check(
        "AC103 open already-visible B is a no-op",
        s.paneA === "host-a" && s.paneB === "host-b",
        JSON.stringify(s),
      ),
    );
  }

  {
    const s0 = { mode: "split", paneA: "host-a", paneB: "host-b" };
    const s = placeAdditionalRemote(s0, "host-c");
    checks.push(
      check(
        "AC103 A|B + open C replaces B (A stays)",
        s.paneA === "host-a" && s.paneB === "host-c",
        JSON.stringify(s),
      ),
    );
  }

  {
    const s0 = { mode: "split", paneA: "host-a", paneB: "host-b" };
    const focused = focusRemoteInLayout(s0, "host-b");
    const added = focusRemoteInLayout(openSingle(createSftpBrowserState(), "host-a"), "host-b");
    checks.push(
      check(
        "AC103 focusRemoteInLayout keeps A|B; adds B beside A when missing",
        focused.paneA === "host-a" &&
          focused.paneB === "host-b" &&
          added.mode === "split" &&
          added.paneA === "host-a" &&
          added.paneB === "host-b",
        JSON.stringify({ focused, added }),
      ),
    );
  }

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
