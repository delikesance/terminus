# Terminus & Sugarloaf Architecture Documentation

## Agent workflow (mandatory)

### Branches
- **Work on `development`**, never commit day-to-day work straight to `main`.
- If `development` is missing, recreate it from up-to-date `main` and push it.
- Short-lived topic branches are optional; land them back on `development` first.
- Do not revive long-lived integration branches (`feat/rio-integration`, etc.) as the default line of work.

### Promote `development` → `main`
1. Review the full delta: `git diff main...development` and the commit list since divergence.
2. Open a merge request / pull request: `development` into `main`.
3. Merge **only** when that review passes (behavior correct, tests cover the change, scope focused, no secrets).
4. If the review fails, leave `main` untouched and fix on `development`.

### Test-driven development
1. Write **failing** tests that specify the intended behavior.
2. Run them and confirm they fail for the expected reason.
3. Implement code until the tests pass; keep the change set minimal.
4. Refactor only while tests remain green.

Pure documentation or chore with no behavior change may skip TDD; everything else follows red → green.

## Overview
This document describes the two primary systems of the Terminus project:
1. **Sugarloaf**: The high-performance, multi-backend rendering engine serving both the terminal grid and the UI chrome.
2. **Terminus UI**: A paint-free UI logic, state, and hit-testing component system.

## 1. Sugarloaf Rendering Engine

Sugarloaf is the core renderer (found in `sugarloaf/src/`). Instead of a traditional node or layer tree, Sugarloaf operates as an immediate-mode batch renderer that targets multiple backends (Vulkan, Metal, WebGPU, and CPU).

### 1.1 The Rendering Pipeline (`Sugarloaf::render_with_grids`)
The pipeline composites the terminal grid operations alongside custom UI overlays within a single render pass.
```rust
pub fn render_with_grids(
    &mut self,
    grids: &mut [(&mut crate::grid::GridRenderer, crate::grid::GridUniforms)],
)
```
- **Phase 1 (State & Layout):** `self.state.compute_dimensions()` computes window and scale transformations.
- **Phase 2 (Grid & Chrome):** UI overlays (Chrome) and grids are provided to the render function. The backends (`render_vulkan`, `render_metal`, `render_wgpu`, `render_cpu`) consume these buffers.

### 1.2 Graphical Primitives
Sugarloaf draws are driven by primitive structs rather than a hierarchical object tree. The primary shapes are:
- `Rect`: Flat color rectangles.
- `Quad`: Rectangles with background colors and border radii (rounded corners).
- `Text` (`SugarloafFont`, `SugarloafFonts`): Glyph protocol processing via Swash. Rasterized into an atlas.
- `Graphic`: Image overlays and custom mask caches.

To draw a rectangle:
```rust
sugarloaf.rect(None, rect.x, rect.y, rect.width, rect.height, rect.color, depth, order);
```

## 2. Terminus UI Component System

The component system (found in `crates/terminus-ui/src/`) manages state, layout, and hit-testing **without** any explicit dependencies on Sugarloaf or the GPU stack.

### 2.1 Separation of Concerns 
Terminus UI provides geometric shapes (`Rect`) and state structs (e.g., `Chrome`, `HostPanel`, `ActivityBarState`), answering *where* elements are and *what* responds to input:
- **Geometry:** Functions like `panel.search_rect(origin_y)` compute layout mathematically in local coordinate spaces.
- **Hit-testing:** Resolves input using plain mathematical point-in-rect tests (`Chrome::handle_press`, `handle_hover`, etc.).

### 2.2 Integration via Rioterm (The Painter)
The actual rasterization bridge lives in `frontends/rioterm/src/renderer/chrome.rs`. It queries the `terminus-ui` state and issues immediate-mode draw commands to `Sugarloaf`. All `paint_x` helper functions link a UI logical element to raw shapes.
```rust
// Example from frontends/rioterm/src/renderer/chrome.rs
pub(crate) fn paint_flat(
    sugarloaf: &mut Sugarloaf,
    rect: &Rect,
    color: [f32; 4],
    depth: f32,
    order: u8,
) {
    // Bridges the UI Math object directly into the Sugarloaf primitive rendering
    sugarloaf.rect(None, rect.x, rect.y, rect.width, rect.height, color, depth, order);
}
```

### 2.3 Vector Icons and SVG Assets
Icons are defined as raw SVG paths in `crates/terminus-ui/src/icons.rs`. Instead of using quad strings to emulate strokes, they are rasterized into coverage masks using `tiny-skia` (handled inside `frontends/rioterm/src/renderer/chrome.rs` `rasterize_icon`) and cached into the Sugarloaf glyph atlas. This handles correct antialiasing at the pixel level.

## References

- Terminus UI integration: `crates/terminus-ui/src/lib.rs`
- Canvas Rasterization (`Chrome.rs`): `frontends/rioterm/src/renderer/chrome.rs`
- Engine Core: `sugarloaf/src/sugarloaf.rs`
