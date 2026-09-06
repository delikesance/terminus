import { parseHostPattern, parseKnownHosts } from "./knownHostsParse.js";

function assert(cond, msg) {
  if (!cond) throw new Error(msg);
}

function assertEq(a, b, msg) {
  const as = JSON.stringify(a);
  const bs = JSON.stringify(b);
  if (as !== bs) throw new Error(`${msg}: got ${as} want ${bs}`);
}

const cases = [];

cases.push(["bare_host_default_port", () => {
  assertEq(parseHostPattern("example.com"), { hostname: "example.com", port: 22 }, "bare");
}]);

cases.push(["bracket_port", () => {
  assertEq(parseHostPattern("[192.168.1.10]:2222"), { hostname: "192.168.1.10", port: 2222 }, "bracket");
}]);

cases.push(["hashed_rejected", () => {
  assertEq(parseHostPattern("|1|abc|def|"), null, "hashed");
}]);

cases.push(["happy_path_and_dedupe", () => {
  const text = `
# comment
example.com ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIJustAFakeKeyForUnitTestsOnly012345
[lab.local]:2200 ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABgQC7fakeKeyMaterialForTestsOnlyNothingReal
example.com ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIDuplicateShouldBeIgnoredXXXXXXXXXX
`;
  const r = parseKnownHosts(text);
  assertEq(r.errors, 0, "errors");
  assertEq(r.hosts.length, 2, "host count");
  assertEq(r.hosts[0], { hostname: "example.com", port: 22 }, "first");
  assertEq(r.hosts[1], { hostname: "lab.local", port: 2200 }, "second");
}]);

cases.push(["fail_closed_bad_lines", () => {
  const text = `
not-enough-fields
@cert-authority *.example.com ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAICertAuthorityNotAHostXXXX
|1|hashed|value| ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIHashedHostCannotRecoverYYYY
good.example ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIValidHostKeyMaterialZZZZZZZZ
garbage line with weird stuff
host-bad not-a-key AAAAC3NzaC1lZDI1NTE5AAAAIBadKeyTypeXXXXXXXXXXXXXXXX
`;
  const r = parseKnownHosts(text);
  assert(r.hosts.length === 1, `expected 1 host got ${r.hosts.length}`);
  assertEq(r.hosts[0].hostname, "good.example", "good host");
  assert(r.errors >= 4, `expected >=4 errors got ${r.errors}`);
}]);

cases.push(["comma_hosts", () => {
  const text = `a.example,b.example ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAICommaSeparatedHostsAAAAAAAA`;
  const r = parseKnownHosts(text);
  assertEq(r.errors, 0, "errors");
  assertEq(r.hosts.map((h) => h.hostname), ["a.example", "b.example"], "names");
}]);

cases.push(["empty_file", () => {
  assertEq(parseKnownHosts(""), { hosts: [], errors: 0 }, "empty");
  assertEq(parseKnownHosts("\n# only comments\n\n"), { hosts: [], errors: 0 }, "comments");
}]);

let failed = 0;
for (const [name, fn] of cases) {
  try {
    fn();
    console.log(`ok  ${name}`);
  } catch (err) {
    failed += 1;
    console.error(`FAIL ${name}: ${err.message || err}`);
  }
}

if (failed) {
  console.error(`\n${failed} failed`);
  process.exit(1);
}
console.log(`\n${cases.length} passed`);
