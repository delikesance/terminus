# Overlay dialogs above underlay UI text

## Problem

Sugarloaf composites **all UI text after all quads**. SFTP (and other chrome)
labels therefore floated on top of modal panels (Edit Host, Settings, Vault,
Connection) even when those dialogs used a higher quad `order`.

Suppressing underlay glyphs while a modal was open fixed the bleed but left
holes (rows under the dialog were not drawn at all). That is wrong: underlay
must keep painting; the modal must draw **on top**, not **instead**.

## Solution

Add a dedicated **overlay compositing layer** in Sugarloaf, drawn after normal
UI text:

1. Grid  
2. Normal quads (`rect` / `quad` / …)  
3. Normal UI text (`draw` / `draw_late` / masks)  
4. **Overlay quads**  
5. **Overlay UI text**

### API

```rust
sugarloaf.begin_overlay();
// modal shell + fields (quads + text_mut draws)
sugarloaf.end_overlay();
```

While overlay mode is active, chrome helpers (`quad` / `rect` / …) and
`text_mut()` draws enqueue into the overlay buffers.

### Call sites

`frontends/rioterm/src/renderer/chrome.rs` wraps:

- Add / Edit Host  
- Settings  
- Vault unlock  
- Connection progress  

SFTP always paints its full glyph set; it never skips rows for stacking.

### Do not

- Skip underlay text/icons that intersect the dialog rect to “make room”
- Rely on quad `order` alone to cover UI text (text always wins over quads)

## Files

| Area | Path |
|------|------|
| Overlay mode + routing | `sugarloaf/src/sugarloaf.rs` |
| Overlay batches | `sugarloaf/src/renderer/compositor.rs` |
| Pass order (wgpu/Metal/Vulkan/CPU) | `sugarloaf/src/renderer/mod.rs` (+ vulkan/cpu) |
| Overlay text instances | `sugarloaf/src/text.rs` |
| Modal wrappers | `frontends/rioterm/src/renderer/chrome.rs` |
| SFTP underlay (no suppress) | `frontends/rioterm/src/renderer/sftp_pane.rs` |
