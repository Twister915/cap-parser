#[macro_use]
extern crate derivative;

#[cfg(not(target_env = "msvc"))]
use jemallocator::Jemalloc;

#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

use std::fs::File;

mod parser;

use std::io::{Read, Write};
use nom::error::VerboseError;
use crate::parser::renderer::{Handler, Display};
use crate::parser::parse::packet;
use leptess::tesseract::{TessApi, TessInitError};
use leptess::leptonica::pix_read;
use std::path::Path;
use image::ImageError;
use leptess::capi;
use std::ffi::{CStr, CString};

fn main() -> std::io::Result<()> {
    let mut f = File::open("subs.sup")?;
    let mut buffer = Vec::with_capacity(f.metadata()?.len() as usize);
    f.read_to_end(&mut buffer)?;

    let mut fout = File::create("subs.srt")?;
    let text = do_parse(buffer.as_slice());
    let bytes = text.into_bytes();
    let bytes_slice = bytes.as_slice();
    fout.write(bytes_slice)?;

    Ok(())
}

fn do_parse<'a>(i: &'a [u8]) -> String {
    let mut out: String = String::new();
    let mut handler = Handler::new();
    let mut frame = 0;

    let mut rest = i;
    const LANG: &str = "eng";
    let mut tess = TessApi::new(None, LANG).unwrap();
    // tess = unsafe {
    //     capi::TessBaseAPIEnd(tess.raw);
    //     capi::TessBaseAPIDelete(tess.raw);
    //
    //     tess.raw = capi::TessBaseAPICreate();
    //
    //     let re = capi::TessBaseAPIInit2(
    //         tess.raw,
    //         std::ptr::null_mut(),
    //         CString::new(LANG).unwrap().as_ptr(),
    //         capi::TessOcrEngineMode_OEM_TESSERACT_ONLY,
    //     );
    //
    //     if re != 0 {
    //         Err(TessInitError { code: re })
    //     } else {
    //         Ok(tess)
    //     }
    // }.unwrap();

    unsafe {
        capi::TessBaseAPISetPageSegMode(tess.raw, leptess::capi::TessPageSegMode_PSM_SPARSE_TEXT_OSD);
    };

    while !rest.is_empty() {
        match packet::<'a, VerboseError<&'a [u8]>>(&rest) {
            Ok((remains, packet)) => {
                rest = remains;
                match handler.handle(packet) {
                    Ok(image) => {
                        match image {
                            Some(img) => {
                                // println!("generated image {} at ({}, {}) -> ({}, {}) ts: {}, dur: {}",
                                //          frame, img.x, img.y, img.x + w, img.y + h, img.begin_mis, img.dur_mis);
                                // frame = frame + 1;
                                match display_to_text(&mut tess, &frame, &img) {
                                    Ok(data) => {
                                        out = out + &data
                                    }
                                    Err(error) => eprintln!("error {:#?}\n", error)
                                }
                                frame = frame + 1;
                            }
                            None => {}
                        }
                    }
                    Err(error) => {
                        eprintln!("error! {:#?}\n", error);
                        return "error".to_string();
                    }
                }
            }
            Err(error) => {
                eprintln!("error! {:#?}\n", error);
                return "error".to_string();
            }
        }
    };

    out
}

fn post_process_text(text: String) -> String {
    let mut out = text;
    out = out.replace("|", "I");
    out = out.trim_start().trim_end().to_string();
    out = out.replace("\n", " ");

    out
}

fn display_to_text(tess: &mut TessApi, frame: &u32, d: &Display) -> Result<String, ImageError> {
    let fname = format!("tmp/sub-{}.tiff", frame);
    d.image.save(fname.clone())?;

    let pix = pix_read(Path::new(&fname)).unwrap();
    tess.set_image(&pix);
    unsafe {
        capi::TessBaseAPISetSourceResolution(tess.raw, 120);
    }
    let text = post_process_text(tess.get_utf8_text().unwrap());

    Ok(format!("{}\n{} --> {}\n{}\n\n", frame + 1, format_timestamp_microsec(d.begin_mis), format_timestamp_microsec(d.begin_mis + d.dur_mis), text))
}

fn format_timestamp_microsec(ms: u64) -> String {
    const MS_PER_MICRO: u64 = 1_000;
    const SEC_PER_MICRO: u64 = MS_PER_MICRO * 1_000;
    const MIN_PER_MICRO: u64 = SEC_PER_MICRO * 60;
    const HR_PER_MICRO: u64 = MIN_PER_MICRO * 60;

    let mut ms_remain = ms;

    let hours = ms_remain / HR_PER_MICRO;
    ms_remain = ms_remain % HR_PER_MICRO;

    let minutes = ms_remain / MIN_PER_MICRO;
    ms_remain = ms_remain % MIN_PER_MICRO;

    let seconds = ms_remain / SEC_PER_MICRO;
    ms_remain = ms_remain % SEC_PER_MICRO;

    let milliseconds = ms_remain / MS_PER_MICRO;

    return format!("{}:{}:{},{}", hours, minutes, seconds, milliseconds);
}