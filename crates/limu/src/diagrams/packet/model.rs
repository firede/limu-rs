//! The model of a packet diagram, as Pele's model.ts makes it.

use super::parser::PacketAst;
use crate::detect::DiagramType;
use crate::errors::{Error, ErrorCode};
use crate::js::chars::{drop_indents, trim_start};
use crate::js::math::min;
use crate::js::number::to_string;
use crate::value::{Map, Value};

/// A field, or the piece of one that falls in a row, Pele's PacketBlock.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct PacketBlock {
    pub(crate) start: f64,
    pub(crate) end: f64,
    /// The bits as Mermaid counts them: one less than the width for the pieces of a field
    /// split across rows.
    pub(crate) bits: f64,
    pub(crate) label: String,
}

/// A parsed packet diagram, Pele's PacketModel.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct PacketModel {
    pub(crate) title: Option<String>,
    pub(crate) acc_title: Option<String>,
    pub(crate) acc_descr: Option<String>,
    pub(crate) bits_per_row: f64,
    pub(crate) rows: Vec<Vec<PacketBlock>>,
}

/// The most rows a packet has, as Mermaid stops at.
const MAX_ROWS: usize = 10_000;

/// A number of the packet's configuration where it is at least least, at most most, or
/// fallback, as Pele's packetOption reads it.
pub(crate) fn packet_option(config: &Map, key: &str, fallback: f64, least: f64, most: f64) -> f64 {
    match config
        .get("packet")
        .and_then(Value::as_object)
        .and_then(|s| s.get(key))
    {
        Some(&Value::Number(v)) if v >= least => min(v, most),
        _ => fallback,
    }
}

fn invalid(message: String) -> Error {
    Error {
        diagram: Some(DiagramType::Packet),
        ..Error::new(message, ErrorCode::Semantic)
    }
}

/// Makes the model of a packet's tree, as Pele's buildPacket does: each field starts where
/// the one before it ends, and is split where the rows end. A title the text gives takes the
/// place of the one its front matter gives, unless it is empty, as `||` has it.
pub(crate) fn build_packet(
    ast: PacketAst,
    config: &Map,
    title: Option<&str>,
) -> Result<PacketModel, Error> {
    let num = to_string;
    let bits_per_row = packet_option(config, "bitsPerRow", 32.0, 1.0, 4096.0).floor();
    let mut rows: Vec<Vec<PacketBlock>> = Vec::new();
    let mut last_bit = -1.0;
    let mut word: Vec<PacketBlock> = Vec::new();
    let mut row = 1.0;

    for block in ast.blocks {
        if let (Some(start), Some(end)) = (block.start, block.end)
            && end < start
        {
            return Err(invalid(format!(
                "Packet block {} - {} is invalid. End must be greater than start.",
                num(start),
                num(end)
            )));
        }
        let mut start = block.start.unwrap_or(last_bit + 1.0);
        if start != last_bit + 1.0 {
            return Err(invalid(format!(
                "Packet block {} - {} is not contiguous. It should start from {}.",
                num(start),
                num(block.end.unwrap_or(start)),
                num(last_bit + 1.0)
            )));
        }
        if block.bits == Some(0.0) {
            return Err(invalid(format!(
                "Packet block {} is invalid. Cannot have a zero bit field.",
                num(start)
            )));
        }
        let end = block.end.unwrap_or(start + block.bits.unwrap_or(1.0) - 1.0);
        let mut bits = block.bits.unwrap_or(end - start + 1.0);
        last_bit = end;

        while word.len() as f64 <= bits_per_row + 1.0 && rows.len() < MAX_ROWS {
            let row_end = row * bits_per_row - 1.0;
            if end <= row_end {
                word.push(PacketBlock {
                    start,
                    end,
                    bits,
                    label: block.label.clone(),
                });
                if end == row_end {
                    rows.push(std::mem::take(&mut word));
                    row += 1.0;
                }
                break;
            }
            word.push(PacketBlock {
                start,
                end: row_end,
                bits: row_end - start,
                label: block.label.clone(),
            });
            rows.push(std::mem::take(&mut word));
            row += 1.0;
            start = row_end + 1.0;
            bits = end - start;
        }
    }
    if !word.is_empty() {
        rows.push(word);
    }

    let given = |s: Option<String>| s.filter(|s| !s.is_empty());
    Ok(PacketModel {
        title: given(ast.title).or_else(|| title.map(str::to_string)),
        acc_title: given(ast.acc_title).map(|s| trim_start(&s).to_string()),
        acc_descr: given(ast.acc_descr).map(|s| drop_indents(&s)),
        bits_per_row,
        rows,
    })
}
