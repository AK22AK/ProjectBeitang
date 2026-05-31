use gpui::*;

pub struct TaskTypography;

impl TaskTypography {
    pub const SYSTEM_FONT_FAMILY: &'static str = ".SystemUIFont";
    pub const PAGE_TITLE_SIZE: f32 = 22.0;
    pub const TASK_TITLE_SIZE: f32 = 14.0;
    pub const META_SIZE: f32 = 12.0;
    pub const DETAIL_TITLE_SIZE: f32 = 19.0;

    pub const BODY_WEIGHT: u16 = 400;
    pub const TASK_TITLE_WEIGHT: u16 = 450;
    pub const SELECTED_TASK_TITLE_WEIGHT: u16 = 500;
    pub const DETAIL_TITLE_WEIGHT: u16 = 520;
    pub const MAX_DEFAULT_WEIGHT: u16 = 520;

    pub fn task_title_font_weight(is_selected: bool) -> FontWeight {
        if is_selected {
            FontWeight::MEDIUM
        } else {
            FontWeight::NORMAL
        }
    }

    pub fn heading_font_weight() -> FontWeight {
        FontWeight::MEDIUM
    }
}

pub struct ClaudeLikeColors;

impl ClaudeLikeColors {
    pub fn app_background() -> Hsla {
        rgb(0xffffff).into()
    }

    pub fn sidebar_background() -> Hsla {
        rgb(0xfbfbfa).into()
    }

    pub fn detail_background() -> Hsla {
        rgb(0xfdfdfc).into()
    }

    pub fn selected_surface() -> Hsla {
        rgb(0xf1f1ef).into()
    }

    pub fn hover_surface() -> Hsla {
        rgb(0xf6f6f4).into()
    }

    pub fn separator() -> Hsla {
        rgb(0xe6e6e3).into()
    }

    pub fn stronger_separator() -> Hsla {
        rgb(0xdedbd4).into()
    }

    pub fn accent() -> Hsla {
        rgb(0xd97757).into()
    }

    pub fn accent_surface() -> Hsla {
        rgb(0xf7eee8).into()
    }

    pub fn danger() -> Hsla {
        rgb(0xc65f45).into()
    }

    pub fn text_primary() -> Hsla {
        rgb(0x242421).into()
    }

    pub fn text_secondary() -> Hsla {
        rgb(0x777771).into()
    }

    pub fn text_tertiary() -> Hsla {
        rgb(0x8c8c86).into()
    }
}
