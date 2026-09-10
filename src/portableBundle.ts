/** Policy for install-and-play native libs: vendor GSSAPI/krb5, not distro Depends. */

const VENDOR_PREFIXES = [
  "libgssapi_krb5",
  "libkrb5support",
  "libk5crypto",
  "libcom_err",
  "libkeyutils",
  "libverto",
];

export type LddEntry = { soname: string; path: string | null };

export function shouldVendorSoname(soname: string): boolean {
  const s = soname.toLowerCase();
  if (s.startsWith("libkrb5.so") || s === "libkrb5") return true;
  if (s.startsWith("libgssapi.so")) return true;
  return VENDOR_PREFIXES.some(
    (p) => s === p || s.startsWith(`${p}.`) || s.startsWith(`${p}.so`),
  );
}

export function parseLddMapping(ldd: string): LddEntry[] {
  const out: LddEntry[] = [];
  for (const raw of ldd.split("\n")) {
    const line = raw.trim();
    if (!line || line.startsWith("linux-vdso")) continue;
    const m = line.match(/^(\S+)\s+=>\s+(\S+)/);
    if (!m) continue;
    const soname = m[1];
    const rest = m[2];
    out.push({
      soname,
      path: rest === "not" ? null : rest,
    });
  }
  return out;
}

export function lddEntriesToVendor(entries: LddEntry[]): { soname: string; path: string }[] {
  const out: { soname: string; path: string }[] = [];
  for (const e of entries) {
    if (!shouldVendorSoname(e.soname) || !e.path) continue;
    out.push({ soname: e.soname, path: e.path });
  }
  return out;
}

export function parseReadelfDynamic(text: string): { needed: string[]; rpath: string } {
  const needed: string[] = [];
  const rpaths: string[] = [];
  for (const line of text.split("\n")) {
    const need = line.match(/\(NEEDED\).*\[([^\]]+)\]/);
    if (need) needed.push(need[1]);
    const rp = line.match(/\((?:RUNPATH|RPATH)\).*\[([^\]]+)\]/);
    if (rp) rpaths.push(rp[1]);
  }
  return { needed, rpath: rpaths.join(":") };
}

export function linuxPortableViolations(audit: {
  needed: string[];
  rpath: string;
  bundled: string[];
}): string[] {
  const violations: string[] = [];
  const vendorNeeded = audit.needed.filter(shouldVendorSoname);
  const hasOrigin = audit.rpath.split(/[:;]/).some((p) => p.includes("$ORIGIN"));
  if (vendorNeeded.length > 0 && !hasOrigin) {
    violations.push("missing $ORIGIN rpath");
  }
  for (const soname of vendorNeeded) {
    const bundled = audit.bundled.some((f) => f === soname || f.startsWith(`${soname}.`));
    if (!bundled) violations.push(`unbundled ${soname}`);
  }
  return violations;
}

export function parseOtoolL(text: string): string[] {
  const out: string[] = [];
  for (const raw of text.split("\n")) {
    const line = raw.trim();
    if (!line) continue;
    if (
      line.startsWith("/") ||
      line.includes(".dylib") ||
      line.includes(".framework")
    ) {
      out.push(line.split(/\s+\(/)[0]);
    }
  }
  return out;
}

export function macosGssViolations(linked: string[]): string[] {
  const violations: string[] = [];
  const mit = linked.filter(
    (s) =>
      /libgssapi_krb5/i.test(s) ||
      ((/homebrew|\/usr\/local\/opt\/krb5/i.test(s) && /gssapi|krb5/i.test(s))),
  );
  if (mit.length) {
    violations.push("must link Apple GSS.framework, not MIT/Homebrew gssapi");
  }
  if (!linked.some((s) => /GSS\.framework/i.test(s))) {
    violations.push("missing GSS.framework");
  }
  return violations;
}

export function windowsGssViolations(imports: string[]): string[] {
  return imports
    .filter((i) => /gssapi|libkrb5|kerberos/i.test(i))
    .map((i) => `unexpected GSS import ${i}`);
}

export function releaseYamlInstallsKrb5Toolchain(yaml: string): boolean {
  const unfolded = yaml.replace(/\\\n/g, " ");
  const installs = [...unfolded.matchAll(/apt-get install[^\n]*/g)].map((m) => m[0]);
  const blob = installs.join(" ");
  return blob.includes("libkrb5-dev") && blob.includes("libclang-dev") && /\bclang\b/.test(blob);
}

export function tauriConfVendorsLinuxGssapi(conf: unknown): boolean {
  const c = conf as {
    build?: { beforeBundleCommand?: string };
    bundle?: {
      linux?: {
        deb?: { files?: Record<string, string> };
        appimage?: { files?: Record<string, string> };
      };
    };
  };
  const cmd = String(c.build?.beforeBundleCommand ?? "");
  if (!cmd.includes("stage-gssapi-libs")) return false;
  const deb = c.bundle?.linux?.deb?.files?.["/usr/lib/terminus"];
  const app = c.bundle?.linux?.appimage?.files?.["/usr/lib/terminus"];
  return Boolean(deb) && Boolean(app);
}

export function tauriConfDoesNotDependOnDistroGssapi(conf: unknown): boolean {
  const c = conf as { bundle?: { linux?: { deb?: { depends?: string[] } } } };
  const depends = c.bundle?.linux?.deb?.depends ?? [];
  return !depends.some((d) => /gssapi|krb5/i.test(d));
}

export function macosJobsForceAppleGssOnMacOnly(yaml: string): boolean {
  const parts = yaml.split(/^jobs:/m);
  if (parts.length > 1 && /LIBGSSAPI_IMPL/.test(parts[0] ?? "")) return false;
  return /runner\.os == ['"]macOS['"]/.test(yaml) && /LIBGSSAPI_IMPL=apple/.test(yaml);
}

export function buildRsSetsOriginRpath(src: string): boolean {
  return src.includes("rpath") && src.includes("$ORIGIN");
}

export function cargoTomlGssapiUnixOnly(toml: string): boolean {
  const unix = /\[target\.'cfg\(unix\)'\.dependencies\][^\[]*libgssapi\s*=/.test(toml);
  const windows = /\[target\.'cfg\(windows\)'\.dependencies\][^\[]*libgssapi\s*=/.test(toml);
  const rootDeps = /(?:^|\n)\[dependencies\][^\[]*libgssapi\s*=/.test(toml);
  return unix && !windows && !rootDeps;
}

export function workflowsRunPortableLibChecker(yaml: string): boolean {
  return yaml.includes("scripts/assert-portable-libs.mjs");
}

function u16(bytes: Uint8Array, off: number): number {
  return bytes[off]! | (bytes[off + 1]! << 8);
}

function u32(bytes: Uint8Array, off: number): number {
  return (
    (bytes[off]! |
      (bytes[off + 1]! << 8) |
      (bytes[off + 2]! << 16) |
      (bytes[off + 3]! << 24)) >>>
    0
  );
}

function u64(bytes: Uint8Array, off: number): bigint {
  const lo = BigInt(u32(bytes, off));
  const hi = BigInt(u32(bytes, off + 4));
  return lo + (hi << 32n);
}

function cString(bytes: Uint8Array, off: number): string {
  let end = off;
  while (end < bytes.length && bytes[end] !== 0) end += 1;
  return new TextDecoder().decode(bytes.subarray(off, end));
}

export function parseElfDynamic(bytes: Uint8Array): { needed: string[]; rpath: string } {
  if (bytes.length < 64 || bytes[0] !== 0x7f || bytes[1] !== 0x45 || bytes[2] !== 0x4c || bytes[3] !== 0x46) {
    throw new Error("not ELF");
  }
  if (bytes[4] !== 2 || bytes[5] !== 1) {
    throw new Error("only ELF64LE is supported");
  }
  const phoff = Number(u64(bytes, 32));
  const phentsize = u16(bytes, 54);
  const phnum = u16(bytes, 56);
  const loads: { va: bigint; offset: bigint; filesz: bigint }[] = [];
  let dynOff = 0;
  let dynSz = 0;
  for (let i = 0; i < phnum; i++) {
    const off = phoff + i * phentsize;
    const type = u32(bytes, off);
    const pOffset = u64(bytes, off + 8);
    const pVaddr = u64(bytes, off + 16);
    const pFilesz = u64(bytes, off + 32);
    if (type === 1) {
      loads.push({ va: pVaddr, offset: pOffset, filesz: pFilesz });
    }
    if (type === 2) {
      dynOff = Number(pOffset);
      dynSz = Number(pFilesz);
    }
  }
  if (!dynOff || !dynSz) throw new Error("no PT_DYNAMIC");

  const vaToOff = (va: bigint): number => {
    for (const l of loads) {
      if (va >= l.va && va < l.va + l.filesz) {
        return Number(l.offset + (va - l.va));
      }
    }
    throw new Error("VA not mapped");
  };

  let strtabVa = 0n;
  const neededOffs: bigint[] = [];
  const rpathOffs: bigint[] = [];
  for (let off = dynOff; off + 16 <= dynOff + dynSz; off += 16) {
    const tag = Number(u64(bytes, off));
    const val = u64(bytes, off + 8);
    if (tag === 0) break;
    if (tag === 1) neededOffs.push(val);
    if (tag === 5) strtabVa = val;
    if (tag === 15 || tag === 29) rpathOffs.push(val);
  }
  if (!strtabVa) throw new Error("no DT_STRTAB");
  const strBase = vaToOff(strtabVa);
  const needed = neededOffs.map((o) => cString(bytes, strBase + Number(o)));
  const rpath = rpathOffs.map((o) => cString(bytes, strBase + Number(o))).join(":");
  return { needed, rpath };
}

export function parsePeImportDlls(bytes: Uint8Array): string[] {
  if (bytes.length < 64 || bytes[0] !== 0x4d || bytes[1] !== 0x5a) {
    throw new Error("not PE");
  }
  const peOff = u32(bytes, 0x3c);
  if (cString(bytes, peOff).slice(0, 2) !== "PE") {
    throw new Error("not PE");
  }
  const coff = peOff + 4;
  const nSections = u16(bytes, coff + 2);
  const optSize = u16(bytes, coff + 16);
  const opt = coff + 20;
  const magic = u16(bytes, opt);
  const isPe32Plus = magic === 0x20b;
  const importRvaOff = isPe32Plus ? opt + 120 : opt + 104;
  const importRva = u32(bytes, importRvaOff);
  if (!importRva) return [];
  const sectionStart = opt + optSize;
  const sections: { va: number; offset: number; size: number }[] = [];
  for (let i = 0; i < nSections; i++) {
    const s = sectionStart + i * 40;
    sections.push({
      va: u32(bytes, s + 12),
      size: u32(bytes, s + 16),
      offset: u32(bytes, s + 20),
    });
  }
  const rvaToOff = (rva: number): number => {
    for (const s of sections) {
      if (rva >= s.va && rva < s.va + s.size) return s.offset + (rva - s.va);
    }
    throw new Error("PE RVA not mapped");
  };
  const dlls: string[] = [];
  let desc = rvaToOff(importRva);
  for (;;) {
    const nameRva = u32(bytes, desc + 12);
    const firstThunk = u32(bytes, desc + 16);
    if (!nameRva && !firstThunk) break;
    dlls.push(cString(bytes, rvaToOff(nameRva)));
    desc += 20;
  }
  return dlls;
}
