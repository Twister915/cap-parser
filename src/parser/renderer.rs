use image::{RgbaImage, Rgba, ImageBuffer};
use nom::lib::std::collections::HashMap;
use crate::parser::types::{YCrCbAColor, RLEEntry, CompositionObject, WindowDefinition, Packet, Segment, PresentationComposition, ObjectDefinition, Timestamp};
use std::cmp::{min, max};
use crate::parser::types::CompositionState::EpochStart;

#[derive(Debug, PartialEq, Clone)]
pub struct Display {
    pub image: RgbaImage,
    // microsecond offset for when to show this image
    pub begin_mis: u64,
    // microsecond duration for how long to show this image
    pub dur_mis: u64,

    pub x: u32,
    pub y: u32,
}

#[derive(Derivative)]
#[derivative(Debug)]
pub struct Handler {
    composition: Option<PresentationComposition>,
    comp_objects: HashMap<u16, CompositionObject>,
    #[derivative(Debug = "ignore")]
    palettes: HashMap<u8, [YCrCbAColor; 256]>,
    windows: HashMap<u8, WindowDefinition>,
    object_data: HashMap<u16, ObjectDefinition>,
    begin_at: Option<Timestamp>,
    end_at: Option<Timestamp>,
}

#[derive(Debug, PartialEq, Clone)]
pub enum HandleError {}

impl Handler {
    pub fn new() -> Handler {
        Handler {
            composition: None,
            comp_objects: HashMap::new(),
            palettes: HashMap::new(),
            windows: HashMap::new(),
            object_data: HashMap::new(),
            begin_at: None,
            end_at: None,
        }
    }

    pub fn handle(&mut self, packet: Packet) -> Result<Option<Display>, HandleError> {
        match packet.segment {
            Segment::PresentationCompositionSegment(pcs) => {
                let res = if pcs.state == EpochStart {
                    self.generate_display()
                } else {
                    None
                };

                self.begin_at = match self.begin_at {
                    Some(v) => {
                        if packet.pts < v {
                            Some(packet.pts)
                        } else {
                            Some(v)
                        }
                    }
                    None => {
                        Some(packet.pts)
                    }
                };

                self.end_at = match self.end_at {
                    Some(v) => {
                        if packet.pts > v {
                            Some(packet.pts)
                        } else {
                            Some(v)
                        }
                    }
                    None => {
                        Some(packet.pts)
                    }
                };

                self.composition = Some(pcs.clone());
                for obj in pcs.objects {
                    self.comp_objects.insert(obj.id, obj);
                }

                Ok(res)
            }
            Segment::WindowDefinitionSegment(windows) => {
                for win in windows {
                    self.windows.insert(win.id, win);
                }

                Ok(None)
            }
            Segment::PaletteDefinitionSegment(pds) => {
                match self.palettes.get_mut(&pds.id) {
                    Some(p) => {
                        for entry in pds.entries {
                            p[entry.id as usize] = entry.color;
                        }
                    }
                    None => {
                        let mut p: [YCrCbAColor; 256] = [YCrCbAColor { y: 16, cr: 128, cb: 128, a: 0 }; 256];
                        for entry in pds.entries {
                            p[entry.id as usize] = entry.color;
                        }

                        self.palettes.insert(pds.id.clone(), p);
                    }
                };

                Ok(None)
            }
            Segment::ObjectDefinitionSegment(ods) => {
                self.object_data.insert(ods.id, ods);
                Ok(None)
            }
            Segment::End => {
                Ok(None)
            }
        }
    }

    fn generate_display(&mut self) -> Option<Display> {
        if self.comp_objects.is_empty() {
            return None;
        }

        let pcs = self.composition.as_ref()?;

        let mut img_x: u32 = pcs.width as u32;
        let mut img_y: u32 = pcs.height as u32;
        let mut img_width: u32 = 0;
        let mut img_height: u32 = 0;
        for window in self.windows.values() {
            let x32 = window.x as u32;
            let y32 = window.y as u32;
            img_x = min(img_x, x32);
            img_y = min(img_y, y32);

            let proposed_width = (x32 - img_x) + window.width as u32;
            let proposed_height = (y32 - img_y) + window.height as u32;
            img_width = max(img_width, proposed_width);
            img_height = max(img_height, proposed_height);
        }

        const PADDING_PERCENT_X: f32 = 0.12;
        const PADDING_PERCENT_Y: f32 = 0.03;
        let padding_x = (pcs.width as f32 * PADDING_PERCENT_X) as u32;
        let dx = img_x - max(0, img_x - padding_x);
        img_width = img_width + (2 * dx);
        img_x = img_x - dx;

        let padding_y = (pcs.height as f32 * PADDING_PERCENT_Y) as u32;
        let dy = img_y - max(0, img_y - padding_y);
        img_height = img_height + (2 * dy);
        img_y = img_y - dy;

        let mut img_data = ImageBuffer::<Rgba<u8>, Vec<u8>>::new(img_width, img_height);

        let palette = self.palettes.get(&pcs.palette_id)?;
        for (id, obj) in &self.object_data {
            let comp_obj = self.comp_objects.get(&id)?;
            let window = self.windows.get(&comp_obj.window_id)?;
            let x0: u32 = comp_obj.x as u32 - img_x;
            let y0: u32 = comp_obj.y as u32 - img_y;

            // todo crop
            let win_max_x: u32 = (window.x + window.width) as u32;
            let win_max_y: u32 = (window.y + window.height) as u32;
            let obj_width = obj.width as u32;
            let obj_max_x: u32 = x0 + obj_width;
            let obj_height = obj.height as u32;
            let obj_max_y: u32 = y0 + obj_height;
            let max_x: u32 = min(win_max_x, obj_max_x);
            let max_y: u32 = min(win_max_y, obj_max_y);

            let obj_width_show: u32 = max_x - x0;
            let obj_height_show: u32 = max_y - y0;

            let mut x_offset: u32 = 0;
            let mut y_offset: u32 = 0;
            for d in &obj.data_raw {
                match d {
                    RLEEntry::Single(b) => {
                        let color = ycbcra_to_rgba(&palette[*b as usize]);
                        if x_offset < obj_width_show && y_offset < obj_height_show {
                            img_data.put_pixel(x0 + x_offset, y0 + y_offset, color);
                        }
                        x_offset = x_offset + 1;
                        if x_offset >= obj_width {
                            x_offset = 0;
                            y_offset = y_offset + 1;
                            if y_offset > obj_height {
                                return None;
                            }
                        }
                    }
                    RLEEntry::Repeated { color: b, count } => {
                        let color = ycbcra_to_rgba(&palette[*b as usize]);
                        for _ in 0..*count {
                            if x_offset < obj_width_show && y_offset < obj_height_show {
                                img_data.put_pixel(x0 + x_offset, y0 + y_offset, color);
                            }
                            x_offset = x_offset + 1;
                            if x_offset >= obj_width {
                                x_offset = 0;
                                y_offset = y_offset + 1;
                                if y_offset > obj_height {
                                    return None;
                                }
                            }
                        }
                    }
                    RLEEntry::EndOfLine => {
                        // skip?
                    }
                };
            }
        };

        let begin_at = pts_to_microsec(self.begin_at?);
        let dur = pts_to_microsec(self.end_at?) - begin_at;
        let dis = Display {
            image: img_data,
            begin_mis: begin_at,
            dur_mis: dur,
            x: img_x,
            y: img_y
        };

        self.reset();

        Some(dis)
    }

    fn reset(&mut self) {
        self.palettes.clear();
        self.comp_objects.clear();
        self.object_data.clear();
        self.windows.clear();
        self.composition = None;
        self.begin_at = None;
        self.end_at = None;
    }
}

fn ycbcra_to_rgba(p: &YCrCbAColor) -> Rgba<u8> {
    let y = ((p.y as f64) - 16.0) * 1.164383562;
    let cb = (p.cb as f64) - 128.0;
    let cr = (p.cr as f64) - 128.0;

    let r = (((y + (cr * 1.792741071)) + 0.5) * (255.0 / 235.0)) as u8;
    let g = (((y - (cr * 0.5329093286) - (cb * 0.2132486143)) + 0.5) * (255.0 / 235.0)) as u8;
    let b = (((y + (cb * 2.112401786)) + 0.5) * (255.0 / 235.0)) as u8;

    Rgba::<u8>([r, g, b, p.a])
}

fn pts_to_microsec(ts: Timestamp) -> u64 {
    return (ts as u64 / 9) * 100;
}