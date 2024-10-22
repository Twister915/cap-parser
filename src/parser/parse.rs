extern crate nom;

use crate::parser::types::*;

use self::nom::{
    branch::alt,
    bytes::complete::tag,
    combinator::{flat_map, map, value},
    error::{context, ErrorKind, ParseError},
    multi::{count, many1},
    number::complete::{be_u16, be_u24, be_u32, be_u8},
    sequence::{preceded, tuple},
    IResult, InputTake,
};

#[inline]
fn timestamp<'a, E: ParseError<&'a [u8]>>(i: &'a [u8]) -> IResult<&'a [u8], Timestamp, E> {
    map(be_u32, Timestamp::from)(i)
}

#[inline]
fn bool_byte<'a, E: ParseError<&'a [u8]>>(
    f_val: u8,
    t_val: u8,
) -> impl Fn(&'a [u8]) -> IResult<&'a [u8], bool, E> {
    alt((value(false, tag([f_val])), value(true, tag([t_val]))))
}

fn segment<'a, E: ParseError<&'a [u8]>>(i: &'a [u8]) -> IResult<&'a [u8], Segment, E> {
    let (rest, (seg_type, size)) = tuple((be_u8, be_u16))(i)?;

    match seg_type {
        0x14 => context("pds", seg_pds(size))(rest),
        0x15 => context("ods", seg_ods(size))(rest),
        0x16 => context("pcs", seg_pcs(size))(rest),
        0x17 => context("wds", seg_wds(size))(rest),
        0x80 => Ok((rest, Segment::End)),
        _ => Err(nom::Err::Error(nom::error::make_error(i, ErrorKind::Eof))),
    }
}

fn seg_pds<'a, E: ParseError<&'a [u8]>>(
    size: u16,
) -> impl Fn(&'a [u8]) -> IResult<&'a [u8], Segment, E> {
    map(
        tuple((
            context("id", be_u8),
            context("version", be_u8),
            context("entries", count(seg_pds_entry, usize::from((size - 2) / 5))),
        )),
        |(id, version, entries)| {
            Segment::PaletteDefinition(PaletteDefinition {
                id,
                version,
                entries,
            })
        },
    )
}

fn seg_pds_entry<'a, E: ParseError<&'a [u8]>>(i: &'a [u8]) -> IResult<&'a [u8], PaletteEntry, E> {
    map(
        tuple((
            context("id", be_u8),
            context("y", be_u8),
            context("Cr", be_u8),
            context("Cb", be_u8),
            context("a", be_u8),
        )),
        |(id, y, cr, cb, a)| PaletteEntry {
            id,
            color: YCrCbAColor { y, cr, cb, a },
        },
    )(i)
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

        Ok((
            rest,
            Segment::ObjectDefinition(ObjectDefinition {
                id,
                version,
                is_last_in_sequence: (flag_raw & 0x40) != 0,
                is_first_in_sequence: (flag_raw & 0x80) != 0,
                width,
                height,
                data_raw: rle_data,
            }),
        ))
    }
}

#[inline]
fn rle_data<'a, E: ParseError<&'a [u8]>>(i: &'a [u8]) -> IResult<&'a [u8], Vec<RLEEntry>, E> {
    many1(rle_entry)(i)
}

fn rle_entry<'a, E: ParseError<&'a [u8]>>(i: &'a [u8]) -> IResult<&'a [u8], RLEEntry, E> {
    if i.is_empty() {
        return Err(nom::Err::Error(nom::error::make_error(i, ErrorKind::Eof)));
    }

    let b0 = i[0];
    if b0 != 0 {
        return Ok((&i[1..], RLEEntry::Single(b0)));
    }

    if i.len() < 2 {
        return Err(nom::Err::Error(nom::error::make_error(i, ErrorKind::Eof)));
    }

    let b1 = i[1];
    if b1 == 0 {
        return Ok((&i[2..], RLEEntry::EndOfLine));
    }

    let mut l = (b1 & 0x3F) as u16;
    let mut l_consumed = 1;
    if b1 & 0x40 != 0 {
        if i.len() < 3 {
            return Err(nom::Err::Error(nom::error::make_error(i, ErrorKind::Eof)));
        }

        l_consumed = 2;
        l = l << 8 | i[2] as u16;
    }

    let col = if b1 & 0x80 == 0 {
        0
    } else {
        l_consumed += 1;
        i[l_consumed]
    };

    let rest = &i[(1 + l_consumed)..];
    Ok((
        rest,
        RLEEntry::Repeated {
            color: col,
            count: l,
        },
    ))
}

fn seg_pcs<'a, E: ParseError<&'a [u8]>>(
    _: u16,
) -> impl Fn(&'a [u8]) -> IResult<&'a [u8], Segment, E> {
    map(
        tuple((
            context("width", be_u16),
            context("height", be_u16),
            context("frame_rate", be_u8),
            context("composition_number", be_u16),
            context("state", seg_pcs_cs),
            context("palette_update", bool_byte(0x00, 0x80)),
            context("palette_id", be_u8),
            flat_map(be_u8, |n_obj| {
                count(
                    context("composition_object", composition_object),
                    usize::from(n_obj),
                )
            }),
        )),
        |(w, h, _, cn, s, u, pid, objs)| {
            Segment::PresentationComposition(PresentationComposition {
                width: w,
                height: h,
                number: cn,
                state: s,
                palette_update: u,
                palette_id: pid,
                objects: objs,
            })
        },
    )
}

#[inline]
fn seg_pcs_cs<'a, E: ParseError<&'a [u8]>>(i: &'a [u8]) -> IResult<&'a [u8], CompositionState, E> {
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
        map(
            tuple((
                context("crop_x", be_u16),
                context("crop_y", be_u16),
                context("crop_w", be_u16),
                context("crop_h", be_u16),
            )),
            |(x, y, width, height)| CompositionObjectCrop::Cropped {
                x,
                y,
                width,
                height,
            },
        )(i1)
    } else {
        Ok((i1, CompositionObjectCrop::NotCropped))
    }?;

    Ok((
        i2,
        CompositionObject {
            id: oid,
            window_id: wid,
            x,
            y,
            crop,
        },
    ))
}

fn seg_wds<'a, E: ParseError<&'a [u8]>>(
    size: u16,
) -> impl Fn(&'a [u8]) -> IResult<&'a [u8], Segment, E> {
    map(
        preceded(
            context("num_windows", be_u8),
            context(
                "windows",
                count(context("def", seg_wds_win), usize::from((size - 1) / 9)),
            ),
        ),
        Segment::WindowDefinition,
    )
}

fn seg_wds_win<'a, E: ParseError<&'a [u8]>>(i: &'a [u8]) -> IResult<&'a [u8], WindowDefinition, E> {
    map(
        tuple((
            context("id", be_u8),
            context("x", be_u16),
            context("y", be_u16),
            context("width", be_u16),
            context("height", be_u16),
        )),
        |(id, x, y, width, height)| WindowDefinition {
            id,
            x,
            y,
            width,
            height,
        },
    )(i)
}

pub fn get_packet<'a, E: ParseError<&'a [u8]>>(i: &'a [u8]) -> IResult<&'a [u8], Packet, E> {
    context(
        "packet",
        map(
            preceded(
                tag(b"PG"),
                tuple((
                    context("pts", timestamp),
                    context("dts", timestamp),
                    context("segment", segment),
                )),
            ),
            |(pts, dts, segment)| Packet { pts, dts, segment },
        ),
    )(i)
}
