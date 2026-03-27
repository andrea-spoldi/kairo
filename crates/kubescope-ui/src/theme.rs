#![allow(dead_code)]
use gpui::Hsla;

/// Dark background color.
pub const BACKGROUND: Hsla = Hsla {
    h: 0.0,
    s: 0.0,
    l: 0.09,
    a: 1.0,
};

/// Status colors for pod states.
pub const STATUS_RUNNING: Hsla = Hsla {
    h: 0.36,
    s: 0.65,
    l: 0.45,
    a: 1.0,
};

pub const STATUS_PENDING: Hsla = Hsla {
    h: 0.11,
    s: 0.80,
    l: 0.55,
    a: 1.0,
};

pub const STATUS_FAILED: Hsla = Hsla {
    h: 0.0,
    s: 0.70,
    l: 0.50,
    a: 1.0,
};

pub const STATUS_SUCCEEDED: Hsla = Hsla {
    h: 0.0,
    s: 0.0,
    l: 0.55,
    a: 1.0,
};
