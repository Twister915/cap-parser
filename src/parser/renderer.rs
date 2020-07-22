use std::cmp::{max, min};

use image::{ImageBuffer, Rgba, RgbaImage};
use nom::lib::std::collections::HashMap;

use crate::parser::types::{
    CompositionObject, ObjectDefinition, Packet, PresentationComposition, RLEEntry, Segment,
    Timestamp, WindowDefinition, YCrCbAColor,
};

#[derive(Debug, PartialEq, Clone)]
pub struct Screen {
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
    palettes: HashMap<u8, [Rgba<u8>; 256]>,
    windows: HashMap<u8, WindowDefinition>,
    object_data: HashMap<u16, ObjectDefinition>,
    begin_at: Option<Timestamp>,
    end_at: Option<Timestamp>,
}

#[derive(Debug, PartialEq, Clone)]
pub enum HandleError {
    BadObjectDefinition,
}

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

    pub fn handle(&mut self, packet: Packet) -> Result<Option<Screen>, HandleError> {
        match packet.segment {
            Segment::PresentationCompositionSegment(pcs) => {
                self.begin_at = match self.begin_at {
                    Some(v) => {
                        if packet.pts < v {
                            Some(packet.pts)
                        } else {
                            Some(v)
                        }
                    }
                    None => Some(packet.pts),
                };

                self.end_at = match self.end_at {
                    Some(v) => {
                        if packet.pts > v {
                            Some(packet.pts)
                        } else {
                            Some(v)
                        }
                    }
                    None => Some(packet.pts),
                };

                let res = if pcs.objects.is_empty() {
                    self.generate_display()
                } else {
                    None
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
                let mut p: [Rgba<u8>; 256] = [Rgba::<u8>([0, 0, 0, 0]); 256];
                for entry in pds.entries {
                    p[entry.id as usize] = ycbcra_to_rgba(&entry.color);
                }

                self.palettes.insert(pds.id.clone(), p);

                Ok(None)
            }
            Segment::ObjectDefinitionSegment(ods) => {
                self.verify_object_data(&ods)?;
                self.object_data.insert(ods.id, ods);
                Ok(None)
            }
            Segment::End => Ok(None),
        }
    }

    fn verify_object_data(&self, data: &ObjectDefinition) -> Result<(), HandleError> {
        if rle_total_count(&data.data_raw) != data.width as usize * data.height as usize {
            return Err(HandleError::BadObjectDefinition);
        }

        let comp_obj_option = self.comp_objects.get(&data.id);
        if comp_obj_option.is_none() {
            return Err(HandleError::BadObjectDefinition);
        }

        let comp_obj = comp_obj_option.unwrap();

        let win_option = self.windows.get(&comp_obj.window_id);
        if win_option.is_none() {
            return Err(HandleError::BadObjectDefinition);
        }

        return Ok(());
    }

    fn generate_display(&mut self) -> Option<Screen> {
        if self.comp_objects.is_empty() {
            return None;
        }

        let pcs = self.composition.as_ref()?;

        let mut img_x: u32 = pcs.width as u32;
        let mut img_y: u32 = pcs.height as u32;
        let mut img_width: u32 = 0;
        let mut img_height: u32 = 0;
        for co in &pcs.objects {
            let ods = self.object_data.get(&co.id)?;
            let x32 = co.x as u32;
            let y32 = co.y as u32;
            img_x = min(img_x, x32);
            img_y = min(img_y, y32);

            let proposed_width = (x32 - img_x) + ods.width as u32;
            let proposed_height = (y32 - img_y) + ods.height as u32;
            img_width = max(img_width, proposed_width);
            img_height = max(img_height, proposed_height);
        }

        const PADDING_PERCENT_X: f32 = 0.12;
        const PADDING_PERCENT_Y: f32 = 0.03;
        let padding_x = (pcs.width as f32 * PADDING_PERCENT_X) as u32;
        let dx = img_x - (max(0, img_x as i32 - padding_x as i32) as u32);
        img_width = img_width + (2 * dx);
        img_x = img_x - dx;

        let padding_y = (pcs.height as f32 * PADDING_PERCENT_Y) as u32;
        let dy = img_y - (max(0, img_y as i32 - padding_y as i32) as u32);
        img_height = img_height + (2 * dy);
        img_y = img_y - dy;

        let mut img_data = ImageBuffer::<Rgba<u8>, Vec<u8>>::new(img_width, img_height);

        let palette = self.palettes.get(&pcs.palette_id)?;
        for comp_obj in &pcs.objects {
            let id = comp_obj.id;
            let obj = self.object_data.get(&id)?;
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
                        let color = palette[*b as usize];
                        if x_offset >= obj_width {
                            x_offset = 0;
                            y_offset = y_offset + 1;
                            if y_offset > obj_height {
                                return None;
                            }
                        }

                        if x_offset < obj_width_show && y_offset < obj_height_show {
                            img_data.put_pixel(x0 + x_offset, y0 + y_offset, color);
                        }
                        x_offset = x_offset + 1;
                    }
                    RLEEntry::Repeated { color: b, count } => {
                        let color = palette[*b as usize];
                        for _ in 0..*count {
                            if x_offset >= obj_width {
                                x_offset = 0;
                                y_offset = y_offset + 1;
                                if y_offset > obj_height {
                                    return None;
                                }
                            }

                            if x_offset < obj_width_show && y_offset < obj_height_show {
                                img_data.put_pixel(x0 + x_offset, y0 + y_offset, color);
                            }
                            x_offset = x_offset + 1;
                        }
                    }
                    RLEEntry::EndOfLine => {
                        x_offset = 0;
                        y_offset = y_offset + 1;
                        if y_offset > obj_height {
                            return None;
                        }
                    }
                };
            }
        }

        let begin_at = pts_to_microsec(self.begin_at?);
        let dur = pts_to_microsec(self.end_at?) - begin_at;
        let dis = Screen {
            image: img_data,
            begin_mis: begin_at,
            dur_mis: dur,
            x: img_x,
            y: img_y,
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
    let mut y = p.y as f64;
    let mut cb = p.cb as f64;
    let mut cr = p.cr as f64;

    y -= 16.0;
    cb -= 128.0;
    cr -= 128.0;

    let y1 = y * 1.164383562;

    let rf = y1 + (cr * 1.792741071);
    let gf = y1 - (cr * 0.5329093286) - (cb * 0.2132486143);
    let bf = y1 + (cb * 2.112401786);

    let r = constrain_double_to_byte(rf + 0.5);
    let g = constrain_double_to_byte(gf + 0.5);
    let b = constrain_double_to_byte(bf + 0.5);

    Rgba::<u8>([r, g, b, p.a])
}

fn constrain_double_to_byte(data: f64) -> u8 {
    if data > 255.0 {
        255
    } else if data < 0.0 {
        0
    } else {
        data as u8
    }
}

fn pts_to_microsec(ts: Timestamp) -> u64 {
    return (ts as u64 / 9) * 100;
}

fn rle_total_count(data: &Vec<RLEEntry>) -> usize {
    let mut out: usize = 0;
    for entry in data {
        match entry {
            RLEEntry::Repeated { count: c, color: _ } => {
                out += *c as usize;
            }
            RLEEntry::Single(_) => out += 1,
            RLEEntry::EndOfLine => {}
        };
    }

    out
}
