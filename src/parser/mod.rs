use nom::error::ParseError;
use crate::parser::types::Packet;
use nom::IResult;

use crate::parser::parse::packet_root;

mod parse;
pub mod types;
pub mod renderer;

pub fn parse_stream<'a, E: ParseError<&'a [u8]>>(
    i: &'a [u8],
) -> IResult<&'a [u8], Vec<Packet>, E> {
    let mut data = i;
    let mut packets = Vec::new();

    while !data.is_empty() {
        let (rest, pkt) = packet_root(&data)?;
        println!("packet: {:#?}", pkt);
        packets.push(pkt);
        data = rest;
    }

    Ok((data, packets))
}