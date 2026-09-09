import {
  normalizeSftpPath,
  resolveUnderRoot,
  parentSftpPath,
  parseSftpError,
  sftpDisplayPath,
  logicalFromDisplayPath,
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
assert(threw, "blocks .. above root");

threw = false;
try {
  normalizeSftpPath("/../../etc");
} catch (e) {
  threw = e.kind === "SftpPathTraversal";
}
assert(threw, "blocks abs escape above /");

// Full-FS: absolute paths are allowed (no session-root sandbox).
assert(resolveUnderRoot("/home/user", "/etc/passwd") === "/etc/passwd", "abs allowed");
assert(resolveUnderRoot("/home/user", "docs/../docs/a") === "/home/user/docs/a", "resolve relative ok");
assert(parentSftpPath("/") === null, "parent root");
assert(parentSftpPath("/a/b") === "/a", "parent abs");
assert(parentSftpPath("a") === ".", "parent rel");

const typed = parseSftpError(
  JSON.stringify({ kind: "SftpTimeout", message: "SFTP operation timed out" }),
);
assert(typed.kind === "SftpTimeout", "parse typed");
assert(parseSftpError("boom").kind === "SftpIo", "plain fallback");

assert(sftpDisplayPath("/home/lab", ".") === "/home/lab", "display cwd");
assert(sftpDisplayPath("/home/lab", "docs") === "/home/lab/docs", "display child");
assert(sftpDisplayPath("/", "docs") === "/docs", "display under /");
assert(sftpDisplayPath("", ".") === "/", "display fallback");
assert(logicalFromDisplayPath("/home/lab", ".", "/home/lab") === "/home/lab", "bar abs home");
assert(logicalFromDisplayPath("/home/lab", ".", "/home/lab/docs") === "/home/lab/docs", "bar abs child");
assert(logicalFromDisplayPath("/home/lab", ".", "docs") === "/home/lab/docs", "bar relative");
assert(logicalFromDisplayPath("/", ".", "/") === "/", "bar fs root");
assert(logicalFromDisplayPath("/", ".", "/etc") === "/etc", "bar under fs root");
assert(logicalFromDisplayPath("/home/lab", ".", "/etc/passwd") === "/etc/passwd", "bar other abs ok");

console.log("sftpPath tests ok");
