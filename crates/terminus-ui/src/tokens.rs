//! Layout and type metrics for the violet-ink design system.
//!
//! Pure constants (logical pixels); colours live in [`crate::theme`].

/// Corner radii.
pub mod radius {
    pub const PILL: f32 = 999.0;
    pub const DIALOG: f32 = 18.0;
    pub const CARD: f32 = 14.0;
    pub const CONTROL: f32 = 10.0;
    pub const SMALL: f32 = 8.0;
    /// Identity tile corner radius as a fraction of the tile size.
    pub const TILE_RATIO: f32 = 0.29;

    /// Corner radius for an identity tile of side `size`.
    pub fn tile(size: f32) -> f32 {
        size * TILE_RATIO
    }
}

/// Control heights.
pub mod height {
    pub const CONTROL_LG: f32 = 44.0;
    pub const CONTROL_MD: f32 = 36.0;
    pub const CONTROL_SM: f32 = 30.0;
    pub const FIELD: f32 = 46.0;
    pub const ROW: f32 = 44.0;
}

/// Identity tile sizes.
pub mod tile {
    pub const SM: f32 = 28.0;
    pub const MD: f32 = 36.0;
}

/// Spacing scale, ascending.
pub mod space {
    pub const XXS: f32 = 2.0;
    pub const XS: f32 = 4.0;
    pub const SM: f32 = 8.0;
    pub const MD: f32 = 12.0;
    pub const LG: f32 = 16.0;
    pub const XL: f32 = 20.0;
    pub const XXL: f32 = 28.0;
    pub const SCALE: [f32; 7] = [XXS, XS, SM, MD, LG, XL, XXL];
}

/// Type sizes.
pub mod font_size {
    pub const DISPLAY: f32 = 28.0;
    pub const TITLE: f32 = 22.0;
    pub const BODY: f32 = 15.0;
    pub const BODY_SM: f32 = 14.0;
    pub const LABEL: f32 = 13.0;
    pub const CAPTION: f32 = 12.0;
    pub const MONO: f32 = 12.0;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spacing_scale_is_strictly_ascending() {
        assert!(space::SCALE.windows(2).all(|w| w[0] < w[1]));
        assert_eq!(space::SCALE, [2.0, 4.0, 8.0, 12.0, 16.0, 20.0, 28.0]);
    }

    #[test]
    fn controls_are_touch_sized_and_ordered() {
        assert!(height::CONTROL_SM >= 30.0);
        assert!(height::CONTROL_SM < height::CONTROL_MD);
        assert!(height::CONTROL_MD < height::CONTROL_LG);
        assert!(height::FIELD >= height::CONTROL_LG);
        assert!(height::ROW >= height::CONTROL_LG);
    }

    #[test]
    fn radii_are_ordered_and_pill_exceeds_any_height() {
        assert!(radius::SMALL < radius::CONTROL);
        assert!(radius::CONTROL < radius::CARD);
        assert!(radius::CARD < radius::DIALOG);
        assert!(radius::PILL > height::FIELD);
    }

    #[test]
    fn tile_radius_follows_ratio() {
        assert!((radius::tile(tile::SM) - 28.0 * 0.29).abs() < 1e-4);
        assert!(radius::tile(tile::MD) > radius::tile(tile::SM));
        assert!(tile::MD <= height::CONTROL_MD);
    }

    #[test]
    fn type_scale_descends_and_is_readable() {
        let sizes = [
            font_size::DISPLAY,
            font_size::TITLE,
            font_size::BODY,
            font_size::BODY_SM,
            font_size::LABEL,
            font_size::CAPTION,
        ];
        assert!(sizes.windows(2).all(|w| w[0] > w[1]));
        assert!(font_size::MONO >= 12.0);
    }
}
