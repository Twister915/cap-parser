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
    PresentationCompositionSegment {
        width: u16,
        height: u16,
        number: u16,
        state: CompositionState,
        palette_update: bool,
        palette_id: u8,
        objects: Vec<CompositionObject>,
    },

    WindowDefinitionSegment(Vec<WindowDefinition>),

    PaletteDefinitionSegment {
        id: u8,
        version: u8,
        entries: Vec<PaletteEntry>,
    },

    #[allow(dead_code)]
    ObjectDefinitionSegment {
        id: u16,
        version: u8,
        is_last_in_sequence: bool,
        is_first_in_sequence: bool,
        width: u16,
        height: u16,
        #[derivative(Debug="ignore")]
        data_raw: RLEData,
    },

    End,
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
    pub color: YCrCbAColor
}

#[derive(Debug, PartialEq, Clone)]
pub struct YCrCbAColor {
    pub y: u8,
    pub cr: u8,
    pub cb: u8,
    pub a: u8,
}

#[derive(Debug, PartialEq, Clone)]
pub enum RLEEntry {
    Single(u8),

    Repeated {
        count: u16,
        color: u8,
    },

    EndOfLine,
}

pub type RLEData = Vec<RLEEntry>;

pub trait RleDecode {

    fn to_byte_lines(&self) -> Vec<Vec<u8>>;
}

impl RleDecode for RLEData {

    fn to_byte_lines(&self) -> Vec<Vec<u8>> {
        let mut lines: Vec<Vec<u8>> = Vec::new();
        let mut line: Vec<u8> = Vec::new();
        for entry in self {
            match entry {
                RLEEntry::Single(b) => {
                    line.push(*b);
                },
                RLEEntry::Repeated{count, color} => {
                    line.resize(line.len() + (*count as usize), *color);
                }
                RLEEntry::EndOfLine => {
                    lines.push(line.clone());
                    line.clear();
                }
            };
        }

        lines
    }
}