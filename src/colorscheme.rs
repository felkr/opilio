use clap::ArgEnum;
use sdl2::pixels::Color;
use std::ops::Deref;
use strum::IntoEnumIterator; // 0.17.1
use strum_macros::{EnumIter, EnumString}; // 0.17.1
#[derive(Clone)]
pub struct ColorScheme {
    pub background: Color,
    pub text: Color,
    pub link: Color,
}
impl Default for ColorScheme {
    fn default() -> ColorScheme {
        ColorScheme {
            background: Color::RGB(255, 255, 255),
            text: Color::RGB(0, 0, 0),
            link: Color::RGB(0, 0, 238),
        }
    }
}
#[derive(EnumString, EnumIter, Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ArgEnum)]
pub enum DefaultColorSchemes {
    Standard,
    Catppuccin,
}
impl DefaultColorSchemes {
    pub fn value(&self) -> ColorScheme {
        match self {
            DefaultColorSchemes::Standard => ColorScheme::default(),
            DefaultColorSchemes::Catppuccin => ColorScheme {
                background: Color::RGB(30, 30, 46),
                text: Color::RGB(217, 224, 238),
                link: Color::RGB(245, 224, 220),
            },
        }
    }
}
