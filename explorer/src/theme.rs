#![allow(unused)]

use iced::{Background, Border, Color, Shadow, Vector, widget::container};

/// Catppuccin Mocha — soothing pastel dark theme.
/// Perfect for developer tools and code editors.
pub mod ctp {
    use iced::Color;

    // ===== Accents =====
    pub const ROSEWATER: Color = Color::from_rgb(0.961, 0.878, 0.863);
    pub const FLAMINGO: Color = Color::from_rgb(0.949, 0.804, 0.804);
    pub const PINK: Color = Color::from_rgb(0.961, 0.761, 0.906);
    pub const MAUVE: Color = Color::from_rgb(0.796, 0.651, 0.969);
    pub const RED: Color = Color::from_rgb(0.953, 0.545, 0.659);
    pub const MAROON: Color = Color::from_rgb(0.922, 0.627, 0.675);
    pub const PEACH: Color = Color::from_rgb(0.980, 0.702, 0.529);
    pub const YELLOW: Color = Color::from_rgb(0.976, 0.886, 0.686);
    pub const GREEN: Color = Color::from_rgb(0.651, 0.890, 0.631);
    pub const TEAL: Color = Color::from_rgb(0.580, 0.886, 0.835);
    pub const SKY: Color = Color::from_rgb(0.537, 0.863, 0.922);
    pub const SAPPHIRE: Color = Color::from_rgb(0.455, 0.780, 0.925);
    pub const BLUE: Color = Color::from_rgb(0.537, 0.706, 0.980);
    pub const LAVENDER: Color = Color::from_rgb(0.706, 0.745, 0.996);

    // ===== Neutrals =====
    pub const TEXT: Color = Color::from_rgb(0.804, 0.839, 0.957);
    pub const SUBTEXT1: Color = Color::from_rgb(0.729, 0.761, 0.871);
    pub const SUBTEXT0: Color = Color::from_rgb(0.651, 0.678, 0.784);
    pub const OVERLAY2: Color = Color::from_rgb(0.576, 0.600, 0.698);
    pub const OVERLAY1: Color = Color::from_rgb(0.498, 0.518, 0.612);
    pub const OVERLAY0: Color = Color::from_rgb(0.424, 0.439, 0.525);
    pub const SURFACE2: Color = Color::from_rgb(0.345, 0.357, 0.439);
    pub const SURFACE1: Color = Color::from_rgb(0.271, 0.278, 0.353);
    pub const SURFACE0: Color = Color::from_rgb(0.192, 0.196, 0.267);
    pub const BASE: Color = Color::from_rgb(0.118, 0.118, 0.180);
    pub const MANTLE: Color = Color::from_rgb(0.094, 0.094, 0.145);
    pub const CRUST: Color = Color::from_rgb(0.067, 0.067, 0.106);
}

/// 6-color accent palette for source↔IR line mapping.
pub const ACCENT: [Color; 6] = [
    ctp::BLUE,
    ctp::GREEN,
    ctp::PEACH,
    ctp::PINK,
    ctp::TEAL,
    ctp::MAUVE,
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
        background: Some(Background::Color(ctp::BASE)),
        border: Border {
            color: ctp::SURFACE0,
            width: 1.0,
            radius: 8.0.into(),
        },
        ..Default::default()
    }
}

pub fn sidebar_panel() -> container::Style {
    container::Style {
        background: Some(Background::Color(ctp::MANTLE)),
        border: Border::default(),
        ..Default::default()
    }
}

pub fn header_bar() -> container::Style {
    container::Style {
        background: Some(Background::Color(ctp::CRUST)),
        border: Border {
            color: ctp::SURFACE0,
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
        background: Some(Background::Color(ctp::SURFACE0)),
        border: Border {
            color: ctp::OVERLAY0,
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
        background: Some(Background::Color(ctp::SURFACE0)),
        border: Border {
            color: ctp::SURFACE1,
            width: 1.0,
            radius: 6.0.into(),
        },
        ..Default::default()
    }
}
