// Copyright (c) 2023-present, Raphael Amorim.
//
// This source code is licensed under the MIT license found in the
// LICENSE file in the root directory of this source tree.
//
// Compositor with vertex capture for text run caching

use crate::renderer::batch::{BatchManager, DrawCmd, QuadInstance};
pub use crate::renderer::batch::{Rect, Vertex};

pub struct Compositor {
    pub batches: BatchManager,
    /// Quads recorded while `Sugarloaf` overlay mode is active.
    /// Composited after normal UI text so modal chrome sits above
    /// underlay glyphs without suppressing them.
    pub overlay_batches: BatchManager,
}

impl Compositor {
    pub fn new() -> Self {
        Self {
            batches: BatchManager::new(),
            overlay_batches: BatchManager::new(),
        }
    }

    #[inline]
    pub fn finish(
        &mut self,
        instances: &mut Vec<QuadInstance>,
        vertices: &mut Vec<Vertex>,
        cmds: &mut Vec<DrawCmd>,
    ) {
        self.batches.build_display_list(instances, vertices, cmds);
        self.batches.reset();
    }

    #[inline]
    pub fn finish_overlay(
        &mut self,
        instances: &mut Vec<QuadInstance>,
        vertices: &mut Vec<Vertex>,
        cmds: &mut Vec<DrawCmd>,
    ) {
        self.overlay_batches
            .build_display_list(instances, vertices, cmds);
        self.overlay_batches.reset();
    }
}

impl Default for Compositor {
    fn default() -> Self {
        Self::new()
    }
}
