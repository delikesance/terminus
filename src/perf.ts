export type RendererKind = "webgl" | "canvas";

export function webglOk(): boolean {
  try {
    const canvas = document.createElement("canvas");
    const gl =
      canvas.getContext("webgl2", { failIfMajorPerformanceCaveat: true }) ||
      canvas.getContext("webgl", { failIfMajorPerformanceCaveat: true });
    return !!gl;
  } catch {
    return false;
  }
}

export function pickRenderer(pref: string): RendererKind {
  if (pref === "webgl" && webglOk()) return "webgl";
  return "canvas";
}
