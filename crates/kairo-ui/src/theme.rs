#![allow(dead_code)]
use gpui::Hsla;

// ── Background ────────────────────────────────────────────────────────────────

/// Dark background color (9% lightness).
pub const BACKGROUND: Hsla = Hsla { h: 0.0, s: 0.0, l: 0.09, a: 1.0 };

// ── Text ─────────────────────────────────────────────────────────────────────
// All values exceed WCAG AA 4.5:1 contrast ratio against BACKGROUND (l=0.09).

/// Primary text — headings, names, values that need full attention.
pub const TEXT_PRIMARY: Hsla = Hsla { h: 0.0, s: 0.0, l: 0.93, a: 1.0 };

/// Secondary text — labels, metadata, supporting information.
pub const TEXT_SECONDARY: Hsla = Hsla { h: 0.0, s: 0.0, l: 0.75, a: 1.0 };

/// Muted text — placeholders, empty-state messages, timestamps.
pub const TEXT_MUTED: Hsla = Hsla { h: 0.0, s: 0.0, l: 0.55, a: 1.0 };

/// Section heading text — visually distinct from body.
pub const TEXT_HEADING: Hsla = Hsla { h: 0.0, s: 0.0, l: 0.85, a: 1.0 };

// ── Surfaces & borders ────────────────────────────────────────────────────────

/// Subtle surface lift for cards and info boxes.
pub const SURFACE: Hsla = Hsla { h: 0.0, s: 0.0, l: 0.13, a: 1.0 };

/// Visible separator / card border (replaces near-invisible 0x3a3a3a).
pub const BORDER: Hsla = Hsla { h: 0.0, s: 0.0, l: 0.28, a: 1.0 };

/// Hover background for interactive rows, tabs, list items.
pub const HOVER_BG: Hsla = Hsla { h: 0.0, s: 0.0, l: 0.16, a: 1.0 };

/// Selected / active tab or item background.
pub const SELECTED_BG: Hsla = Hsla { h: 0.6, s: 0.40, l: 0.22, a: 1.0 };

/// Accent color for the selected-tab bottom border.
pub const ACCENT: Hsla = Hsla { h: 0.6, s: 0.55, l: 0.60, a: 1.0 };

// ── Status colors ─────────────────────────────────────────────────────────────

/// Status colors for pod states.
pub const STATUS_RUNNING: Hsla = Hsla { h: 0.36, s: 0.65, l: 0.55, a: 1.0 };
pub const STATUS_PENDING: Hsla = Hsla { h: 0.11, s: 0.80, l: 0.60, a: 1.0 };
pub const STATUS_FAILED: Hsla = Hsla { h: 0.0, s: 0.70, l: 0.60, a: 1.0 };
pub const STATUS_SUCCEEDED: Hsla = Hsla { h: 0.0, s: 0.0, l: 0.60, a: 1.0 };

/// Map a pod status string to its display color.
pub fn status_color(status: &str) -> Hsla {
    match status {
        "Running" => STATUS_RUNNING,
        "Pending" | "ContainerCreating" | "Initializing" => STATUS_PENDING,
        "Succeeded" | "Completed" => STATUS_SUCCEEDED,
        _ => STATUS_FAILED,
    }
}

/// Unique Unicode symbol per status — conveys state through shape, not just color.
/// This provides a second visual channel for colorblind users.
pub fn status_symbol(status: &str) -> &'static str {
    match status {
        "Running" => "●",
        "Pending" | "ContainerCreating" | "Initializing" => "◐",
        "Succeeded" | "Completed" => "○",
        _ => "✖",
    }
}
