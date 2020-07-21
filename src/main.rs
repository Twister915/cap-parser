#[macro_use]
extern crate derivative;

use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;

use image::ImageError;
#[cfg(not(target_env = "msvc"))]
use jemallocator::Jemalloc;
use leptess::capi;
use leptess::leptonica::pix_read;
use leptess::tesseract::TessApi;
use nom::error::VerboseError;

use crate::parser::parse::packet;
use crate::parser::renderer::{Handler, Screen};
use nom::lib::std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use threadpool::ThreadPool;

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
        fout.write(text.as_bytes())?;

        Ok(())
    })
}

fn do_parse(i: &[u8]) -> String {
    let mut handler = Handler::new();
    let mut frame = 0;

    let mut rest = i;

    let out = Arc::new(Mutex::new(BTreeMap::new()));
    let pool = ThreadPool::new(32);
    while !rest.is_empty() {
        match packet::<VerboseError<&[u8]>>(&rest) {
            Ok((remains, packet)) => {
                rest = remains;
                match handler.handle(packet) {
                    Ok(image) => match image {
                        Some(img) => {
                            let out = out.clone();
                            pool.execute(move || match display_to_text(frame, &img) {
                                Ok(data) => {
                                    out.lock().unwrap().insert(frame, data);
                                }
                                Err(error) => eprintln!("error {:#?}\n", error),
                            });
                            frame = frame + 1;
                        }
                        None => {}
                    },
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
    }
    pool.join();

    let lines: Vec<String> = Arc::try_unwrap(out)
        .unwrap()
        .into_inner()
        .unwrap()
        .into_iter()
        .map(|(_, v)| v)
        .collect();
    lines.join("\n")
}

fn post_process_text(text: String) -> String {
    let mut out = text;
    out = out.replace("|", "I");
    out = out.trim_start().trim_end().to_string();
    out = out.replace("\n", " ");

    out
}

fn display_to_text(frame: u32, d: &Screen) -> Result<String, ImageError> {
    const LANG: &str = "eng";
    // TODO: thread local storage would probably be beneficial here
    let mut tess = TessApi::new(None, LANG).unwrap();

    unsafe {
        capi::TessBaseAPISetPageSegMode(
            tess.raw,
            leptess::capi::TessPageSegMode_PSM_SPARSE_TEXT_OSD,
        );
    };

    let fname = format!("tmp/sub-{}.tiff", frame);
    d.image.save(&fname)?;

    let pix = pix_read(Path::new(&fname)).unwrap();
    tess.set_image(&pix);
    unsafe {
        capi::TessBaseAPISetSourceResolution(tess.raw, 120);
    }
    let text = post_process_text(tess.get_utf8_text().unwrap());

    Ok(format!(
        "{}\n{} --> {}\n{}\n\n",
        frame + 1,
        format_timestamp_microsec(d.begin_mis),
        format_timestamp_microsec(d.begin_mis + d.dur_mis),
        text
    ))
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
