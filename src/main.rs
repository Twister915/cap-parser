#[macro_use]
extern crate derivative;

use fs::File;
use std::fs;
use std::io::{Read, Write};
use std::path::Path;

use image::ImageError;
use leptess::capi;
use leptess::tesseract::TessApi;
use nom::error::VerboseError;

use crate::parser::parse::get_packet;
use crate::parser::renderer::{PacketHandler, Screen};
use nom::lib::std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use threadpool::ThreadPool;

#[cfg(not(target_env = "msvc"))]
use jemallocator::Jemalloc;

#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

mod parser;

fn timeit<Ret, F: FnOnce() -> Ret>(f: F) -> Ret {
    let before = std::time::Instant::now();
    let result = f();
    let after = std::time::Instant::now();
    println!("took {:?}", after - before);

    result
}

fn main() -> std::io::Result<()> {
    timeit(|| {
        let mut f = File::open("subs.sup")?;
        let mut buffer = Vec::with_capacity(f.metadata()?.len() as usize);
        f.read_to_end(&mut buffer)?;

        let mut fout = File::create("subs.srt")?;
        let text = do_parse(&buffer);
        fout.write_all(text.as_bytes())?;

        Ok(())
    })
}

fn do_parse(pgs_buf: &[u8]) -> String {
    let mut packet_handler = PacketHandler::new();
    let mut frame_number = 0;

    let mut rest = pgs_buf;

    let texts = Arc::new(Mutex::new(BTreeMap::new()));
    let thread_pool = ThreadPool::new(num_cpus::get());
    while !rest.is_empty() {
        match get_packet::<VerboseError<&[u8]>>(rest) {
            Ok((remains, packet)) => {
                rest = remains;
                if let Some(value) = handle_packet(
                    &mut packet_handler,
                    packet,
                    &texts,
                    &thread_pool,
                    frame_number,
                ) {
                    return value;
                }
                frame_number += 1;
            }
            Err(error) => {
                eprintln!("error! {:#?}\n", error);
                return "error".to_string();
            }
        }
    }
    thread_pool.join();

    let lines: Vec<String> = Arc::try_unwrap(texts)
        .unwrap()
        .into_inner()
        .unwrap()
        .into_values()
        .collect();
    lines.join("\n")
}

fn handle_packet(
    packet_handler: &mut PacketHandler,
    packet: parser::types::Packet,
    texts: &Arc<Mutex<BTreeMap<u32, String>>>,
    pool: &ThreadPool,
    frame_number: u32,
) -> Option<String> {
    match packet_handler.handle(packet) {
        Ok(image) => {
            if let Some(img) = image {
                let texts = Arc::clone(texts);
                pool.execute(move || match get_text_from_screen(frame_number, &img) {
                    Ok(text) => {
                        if let Some(text) = text {
                            dbg!(&text);
                            texts.lock().unwrap().insert(frame_number, text);
                        };
                    }
                    Err(error) => eprintln!("error {:#?}\n", error),
                });
            }
        }
        Err(error) => {
            eprintln!("error! {:#?}\n", error);
            return Some("error".to_string());
        }
    }
    None
}

fn post_process_text(text: String) -> Option<String> {
    let mut out = text;
    out = out
        .split("\n")
        .map(|line| line.replace("\n", " "))
        .map(|line| line.trim_start().trim_end().to_string())
        .filter(|line| !line.is_empty())
        .collect::<Vec<String>>()
        .join("\n");

    if out.is_empty() {
        None
    } else {
        Some(out)
    }
}

fn get_text_from_screen(frame_num: u32, screen: &Screen) -> Result<Option<String>, ImageError> {
    // save the image to a temporary file
    let temp_subtitle_image_file_path = format!("tmp/sub-{}.tiff", frame_num);
    screen.image.save(&temp_subtitle_image_file_path)?;
    let pix = leptess::leptonica::pix_read(Path::new(&temp_subtitle_image_file_path)).unwrap();

    // TODO: thread local storage would probably be beneficial here to avoid recreating the API
    // object for every screen
    const LANG: &str = "eng";
    let mut tesseract_api = TessApi::new(None, LANG).unwrap();
    let ptr = unsafe { std::ptr::read(&tesseract_api as *const _ as *const usize) as *mut _ };
    unsafe {
        capi::TessBaseAPISetPageSegMode(ptr, leptess::capi::TessPageSegMode_PSM_SPARSE_TEXT_OSD);
    };
    tesseract_api.set_image(&pix);
    unsafe {
        capi::TessBaseAPISetSourceResolution(ptr, 120);
    }

    let text = post_process_text(tesseract_api.get_utf8_text().unwrap());
    match fs::remove_file(&temp_subtitle_image_file_path) {
        Ok(_) => {}
        Err(err) => {
            eprintln!(
                "error deleting frame temp file {} -> {}",
                temp_subtitle_image_file_path, err
            )
        }
    }

    Ok(text.map(|data| {
        format!(
            "{}\n{} --> {}\n{}\n\n",
            frame_num + 1,
            format_timestamp_microsec(screen.begin_us),
            format_timestamp_microsec(screen.begin_us + screen.dur_us),
            data
        )
    }))
}

fn format_timestamp_microsec(ms: u64) -> String {
    const MS_PER_MICRO: u64 = 1_000;
    const SEC_PER_MICRO: u64 = MS_PER_MICRO * 1_000;
    const MIN_PER_MICRO: u64 = SEC_PER_MICRO * 60;
    const HR_PER_MICRO: u64 = MIN_PER_MICRO * 60;

    let mut ms_remain = ms;

    let hours = ms_remain / HR_PER_MICRO;
    ms_remain %= HR_PER_MICRO;

    let minutes = ms_remain / MIN_PER_MICRO;
    ms_remain %= MIN_PER_MICRO;

    let seconds = ms_remain / SEC_PER_MICRO;
    ms_remain %= SEC_PER_MICRO;

    let milliseconds = ms_remain / MS_PER_MICRO;

    format!("{}:{}:{},{}", hours, minutes, seconds, milliseconds)
}
