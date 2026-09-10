import assert from "node:assert/strict";
import { hostAuthUi, parseHostAuthMethod } from "./authMethod.js";

assert.deepEqual(parseHostAuthMethod("gssapi"), { ok: true, method: "gssapi" });
assert.deepEqual(parseHostAuthMethod("GSSAPI"), { ok: true, method: "gssapi" });
assert.deepEqual(parseHostAuthMethod("key"), { ok: true, method: "key" });
assert.deepEqual(parseHostAuthMethod("password"), { ok: true, method: "password" });
assert.equal(parseHostAuthMethod("kerberos").ok, false);
assert.equal(parseHostAuthMethod("ssh").ok, false);

assert.deepEqual(hostAuthUi("gssapi"), {
  segment: "gssapi",
  showKey: false,
  showPassword: false,
});
assert.deepEqual(hostAuthUi("password"), {
  segment: "password",
  showKey: false,
  showPassword: true,
});
assert.deepEqual(hostAuthUi("key"), {
  segment: "key",
  showKey: true,
  showPassword: false,
});
assert.deepEqual(hostAuthUi("agent"), {
  segment: "key",
  showKey: true,
  showPassword: false,
});

console.log("authMethod tests ok");
