//! The sleek, modern purple GUI theme.
//!
//! Both the light and the dark appearance are built on a purple accent palette:
//! soft lavender-tinted surfaces, purple-tinted selections and purple accents.

use egui::style::{Selection, WidgetVisuals, Widgets};
use egui::{Color32, Rounding, Stroke, Visuals};

/// The primary purple accent of the theme.
pub const ACCENT: Color32 = Color32::from_rgb(124, 58, 237); // violet-600

/// Darker purple, used for pressed states and accent borders.
const ACCENT_DARK: Color32 = Color32::from_rgb(96, 42, 168); // violet-700

/// Apply the app-wide sleek purple theme (`dark` selects dark surfaces).
pub fn apply_theme(ctx: &egui::Context, dark: bool) {
    ctx.set_visuals(purple_visuals(dark));
}

/// Build the full custom [`Visuals`] for the sleek purple theme.
pub fn purple_visuals(dark: bool) -> Visuals {
    let base = if dark { Visuals::dark() } else { Visuals::light() };

    let text = if dark {
        Color32::from_rgb(238, 236, 246)
    } else {
        Color32::from_rgb(40, 32, 56)
    };
    let window_fill = if dark {
        Color32::from_rgb(25, 21, 38)
    } else {
        Color32::from_rgb(248, 246, 252)
    };
    let panel_fill = if dark {
        Color32::from_rgb(30, 25, 44)
    } else {
        Color32::from_rgb(243, 238, 247)
    };
    let border = if dark {
        Color32::from_rgb(50, 43, 76)
    } else {
        Color32::from_rgb(226, 216, 244)
    };
    let widget_bg = if dark {
        Color32::from_rgb(38, 33, 54)
    } else {
        Color32::from_rgb(236, 228, 244)
    };

    let widgets = Widgets {
        noninteractive: WidgetVisuals {
            weak_bg_fill: panel_fill,
            bg_fill: panel_fill,
            bg_stroke: Stroke::new(1.0_f32, border),
            fg_stroke: Stroke::new(1.0_f32, text),
            rounding: Rounding::same(6.0),
            expansion: 0.0,
        },
        inactive: WidgetVisuals {
            weak_bg_fill: widget_bg,
            bg_fill: widget_bg,
            bg_stroke: Stroke::new(1.0_f32, if dark {
                Color32::from_rgb(60, 52, 90)
            } else {
                Color32::from_rgb(206, 191, 238)
            }),
            fg_stroke: Stroke::new(1.0_f32, if dark {
                Color32::from_rgb(176, 166, 202)
            } else {
                Color32::from_rgb(84, 62, 116)
            }),
            rounding: Rounding::same(8.0),
            expansion: 0.0,
        },
        hovered: WidgetVisuals {
            weak_bg_fill: if dark {
                Color32::from_rgb(46, 40, 62)
            } else {
                Color32::from_rgb(222, 210, 250)
            },
            bg_fill: if dark {
                Color32::from_rgb(46, 40, 62)
            } else {
                Color32::from_rgb(222, 210, 250)
            },
            bg_stroke: Stroke::new(1.0_f32, if dark {
                Color32::from_rgb(104, 86, 148)
            } else {
                Color32::from_rgb(150, 120, 208)
            }),
            fg_stroke: Stroke::new(1.5_f32, if dark {
                Color32::from_rgb(214, 200, 244)
            } else {
                Color32::from_rgb(74, 48, 110)
            }),
            rounding: Rounding::same(8.0),
            expansion: 1.0,
        },
        active: WidgetVisuals {
            weak_bg_fill: ACCENT,
            bg_fill: ACCENT,
            bg_stroke: Stroke::new(1.0_f32, ACCENT_DARK),
            fg_stroke: Stroke::new(2.0_f32, egui::Color32::WHITE),
            rounding: Rounding::same(8.0),
            expansion: 1.0,
        },
        open: WidgetVisuals {
            weak_bg_fill: if dark {
                Color32::from_rgb(40, 35, 54)
            } else {
                Color32::from_rgb(230, 220, 246)
            },
            bg_fill: if dark {
                Color32::from_rgb(40, 35, 54)
            } else {
                Color32::from_rgb(230, 220, 246)
            },
            bg_stroke: Stroke::new(1.0_f32, if dark {
                Color32::from_rgb(60, 52, 92)
            } else {
                Color32::from_rgb(186, 162, 232)
            }),
            fg_stroke: Stroke::new(1.0_f32, if dark {
                Color32::from_rgb(176, 166, 202)
            } else {
                Color32::from_rgb(84, 62, 116)
            }),
            rounding: Rounding::same(8.0),
            expansion: 0.0,
        },
    };

    Visuals {
        dark_mode: dark,
        override_text_color: Some(text),
        widgets,
        selection: Selection {
            bg_fill: if dark {
                Color32::from_rgb(78, 44, 130)
            } else {
                Color32::from_rgb(232, 216, 250)
            },
            stroke: Stroke::new(1.0_f32, if dark {
                Color32::from_rgb(150, 110, 230)
            } else {
                ACCENT
            }),
        },
        hyperlink_color: if dark {
            Color32::from_rgb(167, 139, 250)
        } else {
            ACCENT
        },
        faint_bg_color: if dark {
            Color32::from_rgb(16, 14, 26)
        } else {
            Color32::from_rgb(242, 236, 246)
        },
        extreme_bg_color: if dark {
            Color32::from_rgb(14, 12, 22)
        } else {
            Color32::from_rgb(251, 249, 254)
        },
        code_bg_color: if dark {
            Color32::from_rgb(15, 12, 20)
        } else {
            Color32::from_rgb(240, 233, 248)
        },
        warn_fg_color: if dark {
            Color32::from_rgb(245, 158, 11)
        } else {
            Color32::from_rgb(217, 119, 6)
        },
        error_fg_color: if dark {
            Color32::from_rgb(248, 113, 113)
        } else {
            Color32::from_rgb(185, 28, 28)
        },
        window_rounding: Rounding::same(10.0),
        window_fill: window_fill,
        window_stroke: Stroke::new(1.0_f32, border),
        menu_rounding: Rounding::same(10.0),
        panel_fill: panel_fill,
        text_cursor: Stroke::new(2.0_f32, if dark {
            Color32::from_rgb(167, 139, 250)
        } else {
            Color32::from_rgb(109, 40, 217)
        }),
        button_frame: true,
        ..base
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn purple_theme_applies_to_both_dark_and_light_mode() {
        let ctx = egui::Context::default();
        apply_theme(&ctx, true);
        let dark = ctx.style().visuals.clone();
        apply_theme(&ctx, false);
        let light = ctx.style().visuals.clone();

        // Dark mode keeps dark purple surfaces, light mode light lavender ones.
        assert!(dark.dark_mode, "dark theme not applied");
        assert!(!light.dark_mode, "light theme not applied");
        assert_eq!(light.window_fill, Color32::from_rgb(248, 246, 252));
        assert_eq!(dark.window_fill, Color32::from_rgb(25, 21, 38));

        // The accent is a purple shade in both modes.
        assert_eq!(ACCENT, Color32::from_rgb(124, 58, 237));
        assert_eq!(dark.selection.stroke.color, Color32::from_rgb(150, 110, 230));
        assert_eq!(light.selection.stroke.color, ACCENT);
    }
}