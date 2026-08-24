//! Adapters between the workstream boundaries.
//!
//! The other workstreams deliberately kept independent copies of a few
//! enums so that `layout`, `render`, `mermaid` and `terminal` never depend on
//! each other. This module owns every conversion between them, so the seams
//! stay in exactly one place:
//!
//! * [`terminal::capabilities::ColorLevel`](crate::terminal::capabilities::ColorLevel)
//!   → [`render::theme::ColorLevel`](crate::render::theme::ColorLevel),
//! * [`terminal::capabilities::ImageSupport`](crate::terminal::capabilities::ImageSupport)
//!   → [`mermaid::ImageCapability`](crate::mermaid::ImageCapability),
//! * `Capabilities` + `MermaidConfig` → [`RenderEnvironment`].

use crate::config::schema::{ColorMode, Config, Theme as ThemeChoice};
use crate::mermaid::mmdc::MmdcRunner;
use crate::mermaid::{ImageCapability, RenderEnvironment};
use crate::render::theme::{ColorLevel as RenderColor, Theme};
use crate::terminal::capabilities::{Capabilities, ColorLevel as TermColor, ImageSupport};

impl From<TermColor> for RenderColor {
    fn from(level: TermColor) -> RenderColor {
        match level {
            TermColor::None => RenderColor::None,
            TermColor::Ansi16 => RenderColor::Ansi16,
            TermColor::Ansi256 => RenderColor::Ansi256,
            TermColor::TrueColor => RenderColor::TrueColor,
        }
    }
}

impl From<ImageSupport> for ImageCapability {
    fn from(support: ImageSupport) -> ImageCapability {
        match support {
            ImageSupport::None => ImageCapability::None,
            ImageSupport::Kitty => ImageCapability::Kitty,
            ImageSupport::Sixel => ImageCapability::Sixel,
            ImageSupport::Iterm2 => ImageCapability::Iterm2,
        }
    }
}

/// The effective colour level for rendering.
///
/// `color = "never"` forces [`RenderColor::None`]; `color = "always"` keeps at
/// least 256 colours even when detection found none (the user asked for it);
/// `auto` uses the detected level.
pub fn color_level(mode: ColorMode, caps: &Capabilities) -> RenderColor {
    match mode {
        ColorMode::Never => RenderColor::None,
        ColorMode::Always => match RenderColor::from(caps.color) {
            RenderColor::None => RenderColor::Ansi256,
            other => other,
        },
        ColorMode::Auto => RenderColor::from(caps.color),
    }
}

/// Resolve the configured theme and downgrade it to the colour level.
pub fn resolve_theme(choice: &ThemeChoice, level: RenderColor) -> Theme {
    Theme::resolve(choice.as_str()).downgraded(level)
}

/// Build the Mermaid [`RenderEnvironment`] from terminal capabilities and the
/// `[mermaid]` configuration.
///
/// `mmdc_available` is only probed when `probe_mmdc` is true; the caller skips
/// the `PATH` scan for documents without diagrams so that startup stays inside
/// the startup budget.
pub fn render_environment(
    cfg: &Config,
    caps: &Capabilities,
    width: usize,
    probe_mmdc: bool,
) -> RenderEnvironment {
    RenderEnvironment {
        images: ImageCapability::from(caps.images),
        mmdc_available: probe_mmdc && MmdcRunner::new(&cfg.mermaid.mmdc_command).is_available(),
        unicode_box: caps.unicode_box,
        width_cells: width.max(1),
        cell_pixels: caps.cell_size(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn color_levels_map_across_the_seam() {
        assert_eq!(RenderColor::from(TermColor::None), RenderColor::None);
        assert_eq!(RenderColor::from(TermColor::Ansi16), RenderColor::Ansi16);
        assert_eq!(RenderColor::from(TermColor::Ansi256), RenderColor::Ansi256);
        assert_eq!(
            RenderColor::from(TermColor::TrueColor),
            RenderColor::TrueColor
        );
    }

    #[test]
    fn image_protocols_map_across_the_seam() {
        assert_eq!(
            ImageCapability::from(ImageSupport::Kitty),
            ImageCapability::Kitty
        );
        assert_eq!(
            ImageCapability::from(ImageSupport::Sixel),
            ImageCapability::Sixel
        );
        assert_eq!(
            ImageCapability::from(ImageSupport::Iterm2),
            ImageCapability::Iterm2
        );
        assert_eq!(
            ImageCapability::from(ImageSupport::None),
            ImageCapability::None
        );
    }

    #[test]
    fn color_mode_overrides_detection() {
        let mut caps = Capabilities {
            color: TermColor::TrueColor,
            ..Capabilities::default()
        };
        assert_eq!(color_level(ColorMode::Never, &caps), RenderColor::None);
        assert_eq!(color_level(ColorMode::Auto, &caps), RenderColor::TrueColor);
        caps.color = TermColor::None;
        assert_eq!(color_level(ColorMode::Always, &caps), RenderColor::Ansi256);
        assert_eq!(color_level(ColorMode::Auto, &caps), RenderColor::None);
    }

    #[test]
    fn environment_skips_the_path_scan_when_not_probing() {
        let cfg = Config::default();
        let caps = Capabilities::default();
        let env = render_environment(&cfg, &caps, 80, false);
        assert!(!env.mmdc_available);
        assert_eq!(env.width_cells, 80);
        assert_eq!(env.cell_pixels, caps.cell_size());
    }
}
