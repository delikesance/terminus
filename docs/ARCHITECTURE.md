# Terminus & Sugarloaf Architecture Documentation

## Overview
This document describes the two primary systems of the Terminus UI:
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
- **Geometry:** Functions like `panel.search_rect(origin_y)` compute layout mathematically.
- **Hit-testing:** Resolves input using plain mathematical point-in-rect tests (`Chrome::handle_press`).

### 2.2 Integration via Rioterm (The Painter)
The actual rasterization bridge lives in `frontends/rioterm/src/renderer/chrome.rs`. It queries the `terminus-ui` state and issues immediate-mode draw commands to `Sugarloaf`. 
```rust
// Example from chrome.rs
pub(crate) fn paint_flat(
    sugarloaf: &mut Sugarloaf,
    rect: &Rect,
    color: [f32; 4],
    depth: f32,
    order: u8,
) {
    sugarloaf.rect(None, rect.x, rect.y, rect.width, rect.height, color, depth, order);
}
```

### 2.3 Vector Icons and SVG Assets
Icons are defined as raw SVG paths in `terminus-ui/src/icons.rs`. Instead of using quad strings to emulate strokes, they are rasterized into coverage masks using `tiny-skia` and cached into the Sugarloaf glyph atlas.

---
*Note: This architecture avoids rigid scene graphs (like traditional layers) to keep the pipeline completely flat, compositing UI over the terminal cells in one pass.*
