pub type Timestamp = u32;

#[derive(Debug, PartialEq, Clone)]
pub struct Packet {
    pub pts: Timestamp,
    pub dts: Timestamp,
    pub segment: Segment,
}

#[derive(Debug, PartialEq, Clone)]
pub enum CompositionState {
    Normal,
    AcquisitionPoint,
    EpochStart,
}

#[derive(Derivative, PartialEq, Clone)]
#[derivative(Debug)]
pub enum Segment {
    PresentationComposition(PresentationComposition),
    WindowDefinition(Vec<WindowDefinition>),
    PaletteDefinition(PaletteDefinition),
    ObjectDefinition(ObjectDefinition),
    End,
}

#[derive(Debug, PartialEq, Clone)]
pub struct PresentationComposition {
    pub width: u16,
    pub height: u16,
    pub number: u16,
    pub state: CompositionState,
    pub palette_update: bool,
    pub palette_id: u8,
    pub objects: Vec<CompositionObject>,
}

#[derive(Debug, PartialEq, Clone)]
pub struct PaletteDefinition {
    pub id: u8,
    pub version: u8,
    pub entries: Vec<PaletteEntry>,
}

#[derive(Derivative, PartialEq, Clone)]
#[derivative(Debug)]
pub struct ObjectDefinition {
    pub id: u16,
    pub version: u8,
    pub is_last_in_sequence: bool,
    pub is_first_in_sequence: bool,
    pub width: u16,
    pub height: u16,
    #[derivative(Debug = "ignore")]
    pub data_raw: RLEData,
}

#[derive(Debug, PartialEq, Clone)]
pub struct CompositionObject {
    pub id: u16,
    pub window_id: u8,
    pub x: u16,
    pub y: u16,
    pub crop: CompositionObjectCrop,
}

#[derive(Debug, PartialEq, Clone)]
pub enum CompositionObjectCrop {
    NotCropped,
    Cropped {
        x: u16,
        y: u16,
        width: u16,
        height: u16,
    },
}

#[derive(Debug, PartialEq, Clone)]
pub struct WindowDefinition {
    pub id: u8,
    pub x: u16,
    pub y: u16,
    pub width: u16,
    pub height: u16,
}

#[derive(Debug, PartialEq, Clone)]
pub struct PaletteEntry {
    pub id: u8,
    pub color: YCrCbAColor,
}

#[derive(Debug, PartialEq, Clone, Copy)]
pub struct YCrCbAColor {
    pub y: u8,
    pub cr: u8,
    pub cb: u8,
    pub a: u8,
}

#[derive(Debug, PartialEq, Clone)]
pub enum RLEEntry {
    Single(u8),

    Repeated { count: u16, color: u8 },

    EndOfLine,
}

pub type RLEData = Vec<RLEEntry>;
