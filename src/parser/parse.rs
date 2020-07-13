extern crate nom;

use self::nom::{
    bytes::complete::tag,
    combinator::{flat_map, map, value},
    error::{ErrorKind, ParseError, context},
    multi::{count, many1},
    number::complete::{be_u8, be_u16, be_u24, be_u32},
    sequence::{preceded, tuple},
    branch::alt,
    InputTake,
    IResult
};

use crate::parser::types::*;

fn timestamp<'a, E: ParseError<&'a [u8]>>(
    i: &'a [u8],
) -> IResult<&'a [u8], Timestamp, E> {
    map(be_u32, Timestamp::from)(i)
}

fn bool_byte<'a, E: ParseError<&'a [u8]>>(
    f_val: u8, t_val: u8,
) -> impl Fn(&'a [u8]) -> IResult<&'a [u8], bool, E> {
    alt((
        value(false, tag([f_val])),
        value(true, tag([t_val])),
    ))
}

fn segment<'a, E: ParseError<&'a [u8]>>(
    i: &'a [u8],
) -> IResult<&'a [u8], Segment, E> {
    let (rest, (seg_type, size)) = tuple((be_u8, be_u16))(i)?;

    match seg_type {
        0x14 => context("pds", seg_pds(size))(rest),
        0x15 => context("ods", seg_ods(size))(rest),
        0x16 => context("pcs", seg_pcs(size))(rest),
        0x17 => context("wds", seg_wds(size))(rest),
        0x80 => Ok((rest, Segment::End)),
        _ => Err(nom::Err::Error(nom::error::make_error(i, ErrorKind::Eof)))
    }
}

fn seg_pds<'a, E: ParseError<&'a [u8]>>(
    size: u16
) -> impl Fn(&'a [u8]) -> IResult<&'a [u8], Segment, E> {
    map(tuple((
        context("id", be_u8),
        context("version", be_u8),
        context("entries", count(seg_pds_entry, usize::from((size - 2) / 5)))
    )), |(id, version, entries)| {
        Segment::PaletteDefinitionSegment { id, version, entries }
    })
}

fn seg_pds_entry<'a, E: ParseError<&'a [u8]>>(
    i: &'a [u8],
) -> IResult<&'a [u8], PaletteEntry, E> {
    map(tuple((
        context("id", be_u8),
        context("y", be_u8),
        context("Cr", be_u8),
        context("Cb", be_u8),
        context("a", be_u8),
    )), |(id, y, cr, cb, a)| {
        PaletteEntry { id, color: YCrCbAColor { y, cr, cb, a } }
    })(i)
}

fn seg_ods<'a, E: ParseError<&'a [u8]>>(
    _size: u16,
) -> impl Fn(&'a [u8]) -> IResult<&'a [u8], Segment, E> {
    move |i: &'a [u8]| {
        let (after_info, (id, version, flag_raw, data_size, width, height)) = tuple((
            context("id", be_u16),
            context("version", be_u8),
            context("last_in_sequence_flag", be_u8),
            context("data_size", be_u24),
            context("width", be_u16),
            context("height", be_u16),
        ))(i)?;

        // - 4 because data_size includes width & height which is 2 * 2 bytes
        let (rest, data_raw) = after_info.take_split((data_size - 4) as usize);
        let (_, rle_data) = rle_data(data_raw)?;

        Ok((&rest, Segment::ObjectDefinitionSegment {
            id,
            version,
            is_last_in_sequence: !((flag_raw & 0x40) == 0),
            is_first_in_sequence: !((flag_raw & 0x80) == 0),
            width,
            height,
            data_raw: rle_data,
        }))
    }
}

fn rle_data<'a, E: ParseError<&'a [u8]>>(
    i: &'a [u8],
) -> IResult<&'a [u8], Vec<RLEEntry>, E> {
    many1(rle_entry)(i)
}

fn rle_entry<'a, E: ParseError<&'a [u8]>>(
    i: &'a [u8],
) -> IResult<&'a [u8], RLEEntry, E> {
    // no bytes, error
    if i.is_empty() {
        Err(nom::Err::Error(nom::error::make_error(i, ErrorKind::Eof)))
    } else {
        let b0 = i[0];
        // first byte is 0, check second byte
        if b0 == 0x0 {
            // no second byte, error
            if i.len() < 2 {
                Err(nom::Err::Error(nom::error::make_error(i, ErrorKind::Eof)))
            } else {
                let b1 = i[1];
                // no third byte?
                if i.len() < 3 {
                    // if the second byte is 0, then this is EOL
                    // otherwise, error
                    if b1 == 0x00 {
                        Ok((&i[2..], RLEEntry::EndOfLine))
                    } else {
                        Err(nom::Err::Error(nom::error::make_error(i, ErrorKind::Eof)))
                    }
                } else {
                    // if 7th bit is 0, then the length is encoded in 6 bits
                    if b1 & 0x40 == 0 {
                        // if b1 is 0 then color is 0
                        // otherwise color is contents of next byte (3rd byte)
                        let (color, skip) = if b1 == 0x00 {
                            (0, 2)
                        } else {
                            (i[2], 3)
                        };

                        let rest = &i[skip..];
                        let count = (b1 as u16) & ((1 << 6) - 1);
                        Ok((rest, RLEEntry::Repeated {
                            count,
                            color,
                        }))
                    } else { // if 7th bit is 1:
                        // length is 2nd byte, except first 2 bits, and 3rd byte
                        let b2 = i[2];
                        let count = (((b1 as u16) & ((1 << 6) - 1)) << 8) | (b2 as u16);

                        // if the 8th bit is 0, then the color is 0
                        // otherwise, the color is encoded in the 4th byte
                        if b1 & 0x80 == 0 {
                            Ok((&i[3..], RLEEntry::Repeated {
                                count,
                                color: 0,
                            }))
                        } else {
                            // if there is no 4th byte, that's an error
                            if i.len() < 4 {
                                Err(nom::Err::Error(nom::error::make_error(i, ErrorKind::Eof)))
                            } else {
                                let color = i[3];
                                Ok((&i[4..], RLEEntry::Repeated {
                                    count,
                                    color,
                                }))
                            }
                        }
                    }
                }
            }
        } else {
            Ok((&i[1..], RLEEntry::Single(i[0])))
        }
    }
}

fn seg_pcs<'a, E: ParseError<&'a [u8]>>(
    _: u16
) -> impl Fn(&'a [u8]) -> IResult<&'a [u8], Segment, E> {
    map(tuple((
        context("width", be_u16),
        context("height", be_u16),
        context("frame_rate", be_u8),
        context("composition_number", be_u16),
        context("state", seg_pcs_cs),
        context("palette_update", bool_byte(0x00, 0x80)),
        context("palette_id", be_u8),
        flat_map(be_u8, |n_obj| {
            count(context("composition_object", composition_object), usize::from(n_obj))
        })
    )), |(w, h, _, cn, s, u, pid, objs)| {
        Segment::PresentationCompositionSegment {
            width: w,
            height: h,
            number: cn,
            state: s,
            palette_update: u,
            palette_id: pid,
            objects: objs,
        }
    })
}

fn seg_pcs_cs<'a, E: ParseError<&'a [u8]>>(
    i: &'a [u8],
) -> IResult<&'a [u8], CompositionState, E> {
    alt((
        value(CompositionState::Normal, tag([0x00])),
        value(CompositionState::AcquisitionPoint, tag([0x40])),
        value(CompositionState::EpochStart, tag([0x80])),
    ))(i)
}

fn composition_object<'a, E: ParseError<&'a [u8]>>(
    i: &'a [u8],
) -> IResult<&'a [u8], CompositionObject, E> {
    let (i1, (oid, wid, is_crop, x, y)) = tuple((
        context("object_id", be_u16),
        context("window_id", be_u8),
        context("is_crop", bool_byte(0x00, 0x40)),
        context("x", be_u16),
        context("y", be_u16),
    ))(i)?;

    let (i2, crop) = if is_crop {
        map(tuple((
            context("crop_x", be_u16),
            context("crop_y", be_u16),
            context("crop_w", be_u16),
            context("crop_h", be_u16))
        ), |(x, y, width, height)| {
            CompositionObjectCrop::Cropped { x, y, width, height }
        })(i1)
    } else {
        Ok((i1, CompositionObjectCrop::NotCropped))
    }?;

    Ok((i2, CompositionObject {
        id: oid,
        window_id: wid,
        x,
        y,
        crop,
    }))
}

fn seg_wds<'a, E: ParseError<&'a [u8]>>(
    size: u16,
) -> impl Fn(&'a [u8]) -> IResult<&'a [u8], Segment, E> {
    map(
        preceded(
            context("num_windows", be_u8),
            context("windows", count(
                context("def", seg_wds_win),
                usize::from((size - 1) / 9)))),
        |w| {
            Segment::WindowDefinitionSegment(w)
        })
}

fn seg_wds_win<'a, E: ParseError<&'a [u8]>>(
    i: &'a [u8],
) -> IResult<&'a [u8], WindowDefinition, E> {
    map(tuple((
        context("id", be_u8),
        context("x", be_u16),
        context("y", be_u16),
        context("width", be_u16),
        context("height", be_u16)
    )), |(id, x, y, width, height)| {
        WindowDefinition { id, x, y, width, height }
    })(i)
}

pub(crate) fn packet_root<'a, E: ParseError<&'a [u8]>>(
    i: &'a [u8],
) -> IResult<&'a [u8], Packet, E> {
    context("packet",
            map(preceded(tag(b"PG"), tuple((
                context("pts", timestamp),
                context("dts", timestamp),
                context("segment", segment)))),
                |(pts, dts, segment)| Packet { pts, dts, segment }))(i)
}