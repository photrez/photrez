use crate::units::DimensionUnit;

use super::{NewDocCategory, NewDocPreset};

const PAPER_PRESETS: &[NewDocPreset] = &[
    NewDocPreset {
        name: "A3",
        width: 3508,
        height: 4961,
    },
    NewDocPreset {
        name: "A4",
        width: 2480,
        height: 3508,
    },
    NewDocPreset {
        name: "A5",
        width: 1748,
        height: 2480,
    },
    NewDocPreset {
        name: "Letter",
        width: 2550,
        height: 3300,
    },
    NewDocPreset {
        name: "Legal",
        width: 2550,
        height: 4200,
    },
    NewDocPreset {
        name: "Foolscap",
        width: 2550,
        height: 3900,
    },
];

const SOCIAL_PRESETS: &[NewDocPreset] = &[
    NewDocPreset {
        name: "Facebook Post",
        width: 1200,
        height: 1200,
    },
    NewDocPreset {
        name: "Facebook Cover",
        width: 1640,
        height: 664,
    },
    NewDocPreset {
        name: "Instagram Square",
        width: 1080,
        height: 1080,
    },
    NewDocPreset {
        name: "Instagram Portrait",
        width: 1080,
        height: 1350,
    },
    NewDocPreset {
        name: "Instagram Story",
        width: 1080,
        height: 1920,
    },
    NewDocPreset {
        name: "X Post",
        width: 1600,
        height: 900,
    },
    NewDocPreset {
        name: "X Header",
        width: 1500,
        height: 500,
    },
    NewDocPreset {
        name: "LinkedIn Banner",
        width: 1584,
        height: 396,
    },
    NewDocPreset {
        name: "TikTok Video",
        width: 1080,
        height: 1920,
    },
    NewDocPreset {
        name: "YouTube Thumbnail",
        width: 1280,
        height: 720,
    },
];

const WEB_PRESETS: &[NewDocPreset] = &[
    NewDocPreset {
        name: "HD",
        width: 1920,
        height: 1080,
    },
    NewDocPreset {
        name: "QHD",
        width: 2560,
        height: 1440,
    },
    NewDocPreset {
        name: "4K",
        width: 3840,
        height: 2160,
    },
];

const PHOTO_PRESETS: &[NewDocPreset] = &[
    NewDocPreset {
        name: "4×6 Landscape",
        width: 1800,
        height: 1200,
    },
    NewDocPreset {
        name: "4×6 Portrait",
        width: 1200,
        height: 1800,
    },
    NewDocPreset {
        name: "5×7 Landscape",
        width: 2100,
        height: 1500,
    },
    NewDocPreset {
        name: "5×7 Portrait",
        width: 1500,
        height: 2100,
    },
    NewDocPreset {
        name: "8×10 Landscape",
        width: 2400,
        height: 3000,
    },
    NewDocPreset {
        name: "8×10 Portrait",
        width: 3000,
        height: 2400,
    },
];

impl NewDocCategory {
    pub const ALL: [Self; 4] = [
        Self::Paper,
        Self::SocialMedia,
        Self::WebVideo,
        Self::Photography,
    ];

    pub fn label_key(self) -> &'static str {
        match self {
            Self::Paper => "dialogs.newDocument.categories.paper",
            Self::SocialMedia => "dialogs.newDocument.categories.socialMedia",
            Self::WebVideo => "dialogs.newDocument.categories.webVideo",
            Self::Photography => "dialogs.newDocument.categories.photography",
        }
    }

    pub fn presets(self) -> &'static [NewDocPreset] {
        match self {
            Self::Paper => PAPER_PRESETS,
            Self::SocialMedia => SOCIAL_PRESETS,
            Self::WebVideo => WEB_PRESETS,
            Self::Photography => PHOTO_PRESETS,
        }
    }
}

/// The document's initial background (`NewDocumentResult.backgroundColor`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum NewDocBackground {
    Transparent,
    #[default]
    White,
}

impl NewDocBackground {
    pub fn label_key(self) -> &'static str {
        match self {
            Self::Transparent => "dialogs.newDocument.transparent",
            Self::White => "dialogs.newDocument.white",
        }
    }
}

/// The text field receiving the dialog's keystrokes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NewDocField {
    Name,
    Width,
    Height,
}

/// The New Document dialog's form state.
#[derive(Clone, Debug, PartialEq)]
pub struct NewDocumentSession {
    pub category: NewDocCategory,
    pub name: String,
    /// Authoritative size, always in pixels.
    pub width_px: u32,
    pub height_px: u32,
    /// Display buffers for the width/height fields in `unit`.
    pub width_text: String,
    pub height_text: String,
    pub unit: DimensionUnit,
    pub background: NewDocBackground,
    pub field: Option<NewDocField>,
    pub unit_open: bool,
    pub background_open: bool,
    /// Last preset press and its time, for the double-click create.
    pub last_preset: Option<(usize, std::time::Instant)>,
}

impl Default for NewDocumentSession {
    fn default() -> Self {
        let unit = DimensionUnit::Px;
        Self {
            category: NewDocCategory::Paper,
            name: "New Project".to_string(), // i18n-exempt: dialog default name
            width_px: 1080,
            height_px: 1080,
            width_text: unit.format(1080),
            height_text: unit.format(1080),
            unit,
            background: NewDocBackground::White,
            field: None,
            unit_open: false,
            background_open: false,
            last_preset: None,
        }
    }
}

impl NewDocumentSession {
    /// `isPresetSelected`: the name and both dimensions must match.
    pub fn preset_selected(&self, preset: &NewDocPreset) -> bool {
        self.name == preset.name && self.width_px == preset.width && self.height_px == preset.height
    }

    /// `applyPreset`: copy the preset's name and size into the form.
    pub fn apply_preset(&mut self, preset: &NewDocPreset) {
        self.name = preset.name.to_string();
        self.width_px = preset.width;
        self.height_px = preset.height;
        self.width_text = self.unit.format(preset.width);
        self.height_text = self.unit.format(preset.height);
    }

    /// Commit on blur: a missing, non-numeric or non-positive value
    /// becomes one pixel; anything else is converted and clamped.
    pub fn commit_dimension(&mut self, field: NewDocField) {
        let (text, width) = match field {
            NewDocField::Width => (&self.width_text, true),
            NewDocField::Height => (&self.height_text, false),
            NewDocField::Name => return,
        };
        let px = match text.trim().parse::<f64>() {
            Ok(value) if value > 0.0 => self.unit.to_px(value).min(16_384),
            _ => 1,
        };
        if width {
            self.width_px = px;
            self.width_text = self.unit.format(px);
        } else {
            self.height_px = px;
            self.height_text = self.unit.format(px);
        }
    }

    /// Switching units commits the typed text first and reformats the stored
    /// pixel value, which the unit picker keeps in sync on every change.
    pub fn set_unit(&mut self, unit: DimensionUnit) {
        self.commit_dimension(NewDocField::Width);
        self.commit_dimension(NewDocField::Height);
        self.unit = unit;
        self.width_text = unit.format(self.width_px);
        self.height_text = unit.format(self.height_px);
        self.unit_open = false;
    }
}
