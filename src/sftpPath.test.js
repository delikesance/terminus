import {
  normalizeSftpPath,
  resolveUnderRoot,
  parentSftpPath,
  parseSftpError,
} from "./sftpPath.js";

function assert(cond, msg) {
  if (!cond) throw new Error(msg);
}

assert(normalizeSftpPath(".") === ".", "dot");
assert(normalizeSftpPath("./a/./b") === "a/b", "collapse");
assert(normalizeSftpPath("/a/./b/../c") === "/a/c", "abs collapse");
assert(normalizeSftpPath("/") === "/", "root");

let threw = false;
try {
  normalizeSftpPath("..");
} catch (e) {
  threw = e.kind === "SftpPathTraversal";
}
assert(threw, "blocks ..");

threw = false;
try {
  normalizeSftpPath("/../../etc");
} catch (e) {
  threw = e.kind === "SftpPathTraversal";
}
assert(threw, "blocks abs escape");

threw = false;
try {
  resolveUnderRoot("/home/user", "../secret");
} catch (e) {
  threw = e.kind === "SftpPathTraversal";
}
assert(threw, "resolve blocks escape");

threw = false;
try {
  resolveUnderRoot(".", "/etc/passwd");
} catch (e) {
  threw = e.kind === "SftpPathTraversal";
}
assert(threw, "relative root rejects absolute");

assert(resolveUnderRoot("/home/user", "docs/../docs/a") === "/home/user/docs/a", "resolve ok");
assert(parentSftpPath("/") === null, "parent root");
assert(parentSftpPath("/a/b") === "/a", "parent abs");
assert(parentSftpPath("a") === ".", "parent rel");

const typed = parseSftpError(
  JSON.stringify({ kind: "SftpTimeout", message: "SFTP operation timed out" }),
);
assert(typed.kind === "SftpTimeout", "parse typed");
assert(parseSftpError("boom").kind === "SftpIo", "plain fallback");

console.log("sftpPath tests ok");
