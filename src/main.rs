#[macro_use]
extern crate derivative;

#[cfg(not(target_env = "msvc"))]
use jemallocator::Jemalloc;

#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

use std::fs::File;

mod parser;

use crate::parser::parse_stream;
use std::io::Read;
use nom::error::VerboseError;

fn main() -> std::io::Result<()> {
    let mut f = File::open("subs.sup")?;
    let mut buffer = Vec::with_capacity(f.metadata()?.len() as usize);
    f.read_to_end(&mut buffer)?;

    let slice = buffer.as_slice();

    match parse_stream::<VerboseError<&[u8]>>(&slice) {
        Ok(_) => {}
        Err(error) => {
            println!("error! {:#?}\n", error)
        }
    };

    Ok(())
}
