#![allow(dead_code)]
use gpui::Hsla;

// ── Backgrounds ───────────────────────────────────────────────────────────────

/// Deepest background (#141414).
pub const BG_BASE: Hsla = Hsla { h: 0.0, s: 0.0, l: 0.078, a: 1.0 };

/// Main surface background (#1b1b1b) — panel backgrounds.
pub const BG_SURFACE: Hsla = Hsla { h: 0.0, s: 0.0, l: 0.106, a: 1.0 };

/// Raised surfaces (#181818) — title bar, sidebars.
pub const BG_RAISED: Hsla = Hsla { h: 0.0, s: 0.0, l: 0.094, a: 1.0 };

/// Inset containers (#151515) — code / log panes.
pub const BG_INSET: Hsla = Hsla { h: 0.0, s: 0.0, l: 0.082, a: 1.0 };

/// Legacy alias — kept for callers not yet migrated.
pub const BACKGROUND: Hsla = BG_BASE;

/// Legacy alias — kept for callers not yet migrated.
pub const SURFACE: Hsla = BG_SURFACE;

// ── Text ─────────────────────────────────────────────────────────────────────

/// Primary text — headings, names, values that need full attention.
pub const TEXT_PRIMARY: Hsla = Hsla { h: 0.0, s: 0.0, l: 0.93, a: 1.0 };

/// Secondary text — labels, metadata, supporting information.
pub const TEXT_SECONDARY: Hsla = Hsla { h: 0.0, s: 0.0, l: 0.75, a: 1.0 };

/// Muted text — placeholders, empty-state messages, timestamps.
pub const TEXT_MUTED: Hsla = Hsla { h: 0.0, s: 0.0, l: 0.55, a: 1.0 };

/// Section heading text — visually distinct from body.
pub const TEXT_HEADING: Hsla = Hsla { h: 0.0, s: 0.0, l: 0.85, a: 1.0 };

// ── Borders ───────────────────────────────────────────────────────────────────

/// Glass border — white/10 (border-white/10).
pub const BORDER: Hsla = Hsla { h: 0.0, s: 0.0, l: 1.0, a: 0.10 };

/// Inner separator — white/6 (subtler than BORDER).
pub const BORDER_SUB: Hsla = Hsla { h: 0.0, s: 0.0, l: 1.0, a: 0.06 };

// ── Interactive ───────────────────────────────────────────────────────────────

/// Hover background for interactive rows, tabs, list items.
pub const HOVER_BG: Hsla = Hsla { h: 0.0, s: 0.0, l: 0.16, a: 1.0 };

/// Selected / active tab or item background.
pub const SELECTED_BG: Hsla = Hsla { h: 0.717, s: 0.40, l: 0.20, a: 1.0 };

// ── Violet accent ─────────────────────────────────────────────────────────────

/// Primary accent — violet-400.
pub const ACCENT: Hsla = Hsla { h: 0.717, s: 0.89, l: 0.63, a: 1.0 };

/// Text on accent backgrounds — violet-100.
pub const ACCENT_FG: Hsla = Hsla { h: 0.728, s: 1.0, l: 0.92, a: 1.0 };

/// Accent fill — violet-400/10.
pub const ACCENT_BG: Hsla = Hsla { h: 0.717, s: 0.89, l: 0.63, a: 0.10 };

/// Accent border — violet-300/20.
pub const ACCENT_BORDER: Hsla = Hsla { h: 0.728, s: 0.87, l: 0.77, a: 0.20 };

// ── Health / pod-status colours ───────────────────────────────────────────────

pub const HEALTH_OK:   Hsla = Hsla { h: 0.36, s: 0.65, l: 0.55, a: 1.0 }; // green
pub const HEALTH_WARN: Hsla = Hsla { h: 0.11, s: 0.80, l: 0.60, a: 1.0 }; // amber
pub const HEALTH_ERR:  Hsla = Hsla { h: 0.0,  s: 0.70, l: 0.60, a: 1.0 }; // red

/// Legacy aliases — kept for callers not yet migrated.
pub const STATUS_RUNNING:   Hsla = HEALTH_OK;
pub const STATUS_PENDING:   Hsla = HEALTH_WARN;
pub const STATUS_FAILED:    Hsla = HEALTH_ERR;
pub const STATUS_SUCCEEDED: Hsla = Hsla { h: 0.0, s: 0.0, l: 0.60, a: 1.0 };

// ── Severity — event cards ────────────────────────────────────────────────────

pub const SEV_CRITICAL_BORDER: Hsla = Hsla { h: 0.0,   s: 0.91, l: 0.64, a: 0.40 };
pub const SEV_CRITICAL_BG:     Hsla = Hsla { h: 0.0,   s: 0.84, l: 0.60, a: 0.10 };
pub const SEV_CRITICAL_FG:     Hsla = Hsla { h: 0.0,   s: 0.87, l: 0.77, a: 1.0  };

pub const SEV_WARNING_BORDER:  Hsla = Hsla { h: 0.114, s: 0.96, l: 0.54, a: 0.40 };
pub const SEV_WARNING_BG:      Hsla = Hsla { h: 0.108, s: 0.96, l: 0.53, a: 0.10 };
pub const SEV_WARNING_FG:      Hsla = Hsla { h: 0.114, s: 0.96, l: 0.73, a: 1.0  };

pub const SEV_INFO_BORDER:     Hsla = Hsla { h: 0.554, s: 0.90, l: 0.60, a: 0.40 };
pub const SEV_INFO_BG:         Hsla = Hsla { h: 0.554, s: 0.89, l: 0.53, a: 0.10 };
pub const SEV_INFO_FG:         Hsla = Hsla { h: 0.554, s: 0.87, l: 0.77, a: 1.0  };

// ── Log level colours ─────────────────────────────────────────────────────────

pub const LOG_ERROR: Hsla = SEV_CRITICAL_FG;
pub const LOG_WARN:  Hsla = SEV_WARNING_FG;
pub const LOG_INFO:  Hsla = TEXT_MUTED;

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Map a pod status string to its display color.
pub fn status_color(status: &str) -> Hsla {
    match status {
        "Running" => HEALTH_OK,
        "Pending" | "ContainerCreating" | "Initializing" => HEALTH_WARN,
        "Succeeded" | "Completed" => STATUS_SUCCEEDED,
        _ => HEALTH_ERR,
    }
}

/// Unique Unicode symbol per status — conveys state through shape, not just color.
pub fn status_symbol(status: &str) -> &'static str {
    match status {
        "Running" => "●",
        "Pending" | "ContainerCreating" | "Initializing" => "◐",
        "Succeeded" | "Completed" => "○",
        _ => "✖",
    }
}
