#![allow(unused)]

use iced::{Background, Border, Color, Shadow, Vector, widget::container};

/// Custom original dark theme palette.
/// Designed for optimal contrast and readability in developer tools.
pub mod palette {
    use iced::Color;

    // ===== Neutrals =====
    pub const BG_MAIN: Color = Color::from_rgb(0.09, 0.10, 0.12);
    pub const BG_SIDEBAR: Color = Color::from_rgb(0.12, 0.13, 0.15);
    pub const BG_HEADER: Color = Color::from_rgb(0.06, 0.07, 0.08);
    pub const BG_SURFACE_0: Color = Color::from_rgb(0.16, 0.17, 0.20);
    pub const BG_SURFACE_1: Color = Color::from_rgb(0.20, 0.22, 0.25);

    pub const TEXT_MAIN: Color = Color::from_rgb(0.90, 0.91, 0.93);
    pub const TEXT_MUTED_0: Color = Color::from_rgb(0.75, 0.77, 0.80);
    pub const TEXT_MUTED_1: Color = Color::from_rgb(0.60, 0.63, 0.68);
    pub const TEXT_DIMMED_1: Color = Color::from_rgb(0.45, 0.48, 0.53);
    pub const TEXT_DIMMED_2: Color = Color::from_rgb(0.35, 0.38, 0.43);

    // ===== Accents =====
    pub const ACCENT_BLUE: Color = Color::from_rgb(0.40, 0.65, 0.95);
    pub const ACCENT_GREEN: Color = Color::from_rgb(0.35, 0.80, 0.55);
    pub const ACCENT_ORANGE: Color = Color::from_rgb(0.95, 0.60, 0.35);
    pub const ACCENT_PINK: Color = Color::from_rgb(0.90, 0.45, 0.75);
    pub const ACCENT_CYAN: Color = Color::from_rgb(0.25, 0.75, 0.85);
    pub const ACCENT_PURPLE: Color = Color::from_rgb(0.70, 0.55, 0.95);
    pub const ACCENT_RED: Color = Color::from_rgb(0.95, 0.35, 0.45);
    pub const ACCENT_YELLOW: Color = Color::from_rgb(0.90, 0.80, 0.40);
}

/// 6-color accent palette for source↔IR line mapping.
pub const ACCENT: [Color; 6] = [
    palette::ACCENT_BLUE,
    palette::ACCENT_GREEN,
    palette::ACCENT_ORANGE,
    palette::ACCENT_PINK,
    palette::ACCENT_CYAN,
    palette::ACCENT_PURPLE,
];

/// Full-opacity block indicator colors.
pub const BLOCK: [Color; 6] = ACCENT;

/// Faded accent for subtle line highlighting.
pub fn accent_faded(idx: usize) -> Color {
    let mut c = ACCENT[idx % ACCENT.len()];
    c.a = 0.10;
    c
}

/// Medium accent for normal line highlighting.
pub fn accent_medium(idx: usize) -> Color {
    let mut c = ACCENT[idx % ACCENT.len()];
    c.a = 0.18;
    c
}

/// Stronger accent for hover states.
pub fn accent_hover(idx: usize) -> Color {
    let mut c = ACCENT[idx % ACCENT.len()];
    c.a = 0.32;
    c
}

/// Very dim accent for non-hovered lines.
pub fn accent_dim(idx: usize) -> Color {
    let mut c = ACCENT[idx % ACCENT.len()];
    c.a = 0.04;
    c
}

// ===== Container style helpers =====

pub fn base_panel() -> container::Style {
    container::Style {
        background: Some(Background::Color(palette::BG_MAIN)),
        border: Border {
            color: palette::BG_SURFACE_0,
            width: 1.0,
            radius: 0.0.into(),
        },
        ..Default::default()
    }
}

pub fn sidebar_panel() -> container::Style {
    container::Style {
        background: Some(Background::Color(palette::BG_SIDEBAR)),
        border: Border::default(),
        ..Default::default()
    }
}

pub fn header_bar() -> container::Style {
    container::Style {
        background: Some(Background::Color(palette::BG_HEADER)),
        border: Border {
            color: palette::BG_SURFACE_0,
            width: 1.0,
            radius: 0.0.into(),
        },
        ..Default::default()
    }
}

pub fn code_line_bg(highlight: Option<Color>) -> container::Style {
    container::Style {
        background: highlight.map(Background::Color),
        border: Border::default(),
        ..Default::default()
    }
}

pub fn tooltip_box() -> container::Style {
    container::Style {
        background: Some(Background::Color(palette::BG_SURFACE_0)),
        border: Border {
            color: palette::TEXT_DIMMED_2,
            width: 1.0,
            radius: 8.0.into(),
        },
        shadow: Shadow {
            color: Color::from_rgba(0.0, 0.0, 0.0, 0.4),
            offset: Vector::new(0.0, 4.0),
            blur_radius: 12.0,
        },
        ..Default::default()
    }
}

pub fn table_header() -> container::Style {
    container::Style {
        background: Some(Background::Color(palette::BG_SURFACE_0)),
        border: Border {
            color: palette::BG_SURFACE_1,
            width: 1.0,
            radius: 6.0.into(),
        },
        ..Default::default()
    }
}
