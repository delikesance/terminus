import assert from "node:assert/strict";
import {
  parseIdentityKind,
  inferIdentityKind,
  parseIdentityKeyError,
} from "./identityKind.js";

assert.deepEqual(parseIdentityKind("key"), { ok: true, kind: "key" });
assert.deepEqual(parseIdentityKind("PASSWORD"), { ok: true, kind: "password" });
assert.deepEqual(parseIdentityKind("agent"), { ok: true, kind: "agent" });
assert.equal(parseIdentityKind("ssh").ok, false);
assert.equal(parseIdentityKind(null).error, "unknown_kind");

assert.equal(inferIdentityKind({ kind: "agent" }), "agent");
assert.equal(inferIdentityKind({ private_key: "-----BEGIN" }), "key");
assert.equal(inferIdentityKind({ passphrase: "secret" }), "password");
assert.equal(inferIdentityKind({}), "key");

assert.deepEqual(parseIdentityKeyError('{"kind":"IdentityKeyInvalid","reason":"bad pem"}'), {
  kind: "IdentityKeyInvalid",
  reason: "bad pem",
});
assert.equal(parseIdentityKeyError("plain boom"), null);
assert.equal(parseIdentityKeyError('{"kind":"HostKeyUnknown"}'), null);

console.log("identityKind tests ok");
