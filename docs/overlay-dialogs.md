# Overlay dialogs above underlay UI text

## Problem

Sugarloaf composites **all UI text after all quads**. SFTP (and other chrome)
labels therefore floated on top of modal panels (Edit Host, Settings, Vault,
Connection) even when those dialogs used a higher quad `order`.

Suppressing underlay glyphs while a modal was open fixed the bleed but left
holes (rows under the dialog were not drawn at all). That is wrong: underlay
must keep painting; the modal must draw **on top**, not **instead**.

A second problem appears when **two** overlay dialogs are open (e.g. Edit Host
plus Unlock Vault): Sugarloaf still has **one** overlay layer, so the frame
becomes `(all overlay quads) → (all overlay text)`. Lower-modal glyphs then
float above the higher modal’s panel.

## Solution

### 1. Overlay compositing layer (Sugarloaf)

Add a dedicated **overlay compositing layer** in Sugarloaf, drawn after normal
UI text:

1. Grid  
2. Normal quads (`rect` / `quad` / …)  
3. Normal UI text (`draw` / `draw_late` / masks)  
4. **Overlay quads**  
5. **Overlay UI text**

```rust
sugarloaf.begin_overlay();
// modal shell + fields (quads + text_mut draws)
sugarloaf.end_overlay();
```

While overlay mode is active, chrome helpers and `text_mut()` draws enqueue
into the overlay buffers.

### 2. Modal paint stack (Terminus chrome)

All open overlay dialogs are painted in **one** `begin_overlay`…`end_overlay`
block, ordered by [`ModalPaintLayer`](../crates/terminus-ui/src/chrome.rs)
(back → front):

1. Connection  
2. Host editor (Add / Edit Host)  
3. Add Snippet  
4. Settings  
5. Vault unlock  

Only the **front** layer emits text/icons (`paint_glyphs = true`). Lower
layers paint scrim + panel shells (and empty field quads for the host editor)
so a translucent top scrim can still dim the form underneath without glyph
bleed.

`Chrome::modal_paint_stack()` / `Chrome::top_modal_paint()` are the source of
truth for that order (hit-testing already prefers vault / settings first).

### Call sites

`frontends/rioterm/src/renderer/chrome.rs` → `paint_modal_stack`.

SFTP always paints its full glyph set in the **normal** layer; it never skips
rows for stacking.

### Do not

- Skip underlay text/icons that intersect the dialog rect to “make room”
- Rely on quad `order` alone to cover UI text (text always wins over quads)
- Open a separate `begin_overlay` per modal and expect later sessions to cover
  earlier modal glyphs (they share one overlay text buffer)

## Files

| Area | Path |
|------|------|
| Overlay mode + routing | `sugarloaf/src/sugarloaf.rs` |
| Overlay batches | `sugarloaf/src/renderer/compositor.rs` |
| Pass order (wgpu/Metal/Vulkan/CPU) | `sugarloaf/src/renderer/mod.rs` (+ vulkan/cpu) |
| Overlay text instances | `sugarloaf/src/text.rs` |
| Modal stack + paint | `frontends/rioterm/src/renderer/chrome.rs` (`paint_modal_stack`) |
| Stack order API | `crates/terminus-ui/src/chrome.rs` (`ModalPaintLayer`) |
| SFTP underlay (no suppress) | `frontends/rioterm/src/renderer/sftp_pane.rs` |
