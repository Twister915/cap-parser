use image::{RgbaImage, Rgba};
use nom::lib::std::collections::HashMap;
use crate::parser::types::{PaletteEntry, YCrCbAColor};

pub struct Display {
    pub image: RgbaImage,
    // microsecond offset for when to show this image
    pub begin_mis: u64,
    // microsecond duration for how long to show this image
    pub dur_mis: u64,

    // position in the frame
    pub x: u16,
    pub y: u16
}

struct Handler {
    palette: HashMap<u8, >
}

fn ycbcr_to_rgb(p: YCrCbAColor) -> Rgba<u8> {

}