//! ZoneMapScan: part-level pruning via zone maps.
//!
//! `can_skip` interprets a `Predicate` against a part's min/max bounds.
//! The operator (below) loops parts, skipping those `can_skip` rejects.

use crate::parser::ast::{CmpOp, Literal, Predicate};
use crate::storage::zone_map::{ZoneEntry, ZoneMap};

/// Returns true if `predicate` is provably false for every row in this part,
/// using only the zone map's min/max bounds. False means "cannot prove —
/// don't skip" (the safe answer; `Filter` still runs row-level).
fn can_skip(zone: &ZoneMap, predicate: &Predicate) -> bool {
    match predicate {
        Predicate::Cmp { col, op, value } => match zone.get(col) {
            None => false,
            Some(column) => column
                .entries
                .iter()
                .all(|e| entry_excludes(column.type_tag, e, op, value)),
        },
        Predicate::And(a, b) => can_skip(zone, a) || can_skip(zone, b),
        Predicate::Or(a, b) => can_skip(zone, a) && can_skip(zone, b),
        Predicate::Not(_) => false,
    }
}

/// Can this one (min, max) entry prove `op value` matches nothing?
fn entry_excludes(type_tag: u8, entry: &ZoneEntry, op: &CmpOp, value: &Literal) -> bool {
    match type_tag {
        1..=4 => match literal_as_i64(value) {
            Some(v) => bounds_exclude(
                i64::from_le_bytes(entry.min_bytes),
                i64::from_le_bytes(entry.max_bytes),
                op,
                v,
            ),
            None => false,
        },
        5..=8 => match literal_as_u64(value) {
            Some(v) => bounds_exclude(
                u64::from_le_bytes(entry.min_bytes),
                u64::from_le_bytes(entry.max_bytes),
                op,
                v,
            ),
            None => false,
        },
        9..=10 => match literal_as_f64(value) {
            Some(v) => bounds_exclude(
                f64::from_bits(u64::from_le_bytes(entry.min_bytes)),
                f64::from_bits(u64::from_le_bytes(entry.max_bytes)),
                op,
                v,
            ),
            None => false,
        },
        _ => false, // bool/str: never in the zone map
    }
}

/// The pruning truth table. Generic so all three families share it.
fn bounds_exclude<T: PartialOrd>(min: T, max: T, op: &CmpOp, value: T) -> bool {
    match op {
        CmpOp::Eq => value < min || value > max,
        CmpOp::Lt => min >= value,
        CmpOp::Le => min > value,
        CmpOp::Gt => max <= value,
        CmpOp::Ge => max < value,
        CmpOp::Ne => false,
    }
}

fn literal_as_i64(v: &Literal) -> Option<i64> {
    match v {
        Literal::Int(i) => Some(*i),
        Literal::UInt(u) => i64::try_from(*u).ok(),
        _ => None,
    }
}

fn literal_as_u64(v: &Literal) -> Option<u64> {
    match v {
        Literal::Int(i) => u64::try_from(*i).ok(),
        Literal::UInt(u) => Some(*u),
        _ => None,
    }
}

fn literal_as_f64(v: &Literal) -> Option<f64> {
    match v {
        Literal::Int(i) => Some(*i as f64),
        Literal::UInt(u) => Some(*u as f64),
        Literal::Float(f) => Some(*f),
        _ => None,
    }
}
use std::path::PathBuf;

use crate::storage::{
    part_discovery::discover_parts,
    schema::{ColumnDef, TableDef},
    zone_map::read_zone_map,
};

use super::{
    batch::Batch,
    full_scan::read_part,
    processor::{ExecutionError, Processor},
};

/// Like FullScan, but skips parts whose zone map proves the predicate false.
///
/// Lazy and sequential: one part is read per `next_batch` call, and parts the
/// zone map rules out are never read from disk at all.
pub struct ZoneMapScan {
    table_dir: PathBuf,
    columns: Vec<ColumnDef>,
    predicate: Predicate,
    part_ids: Vec<u32>,
    cursor: usize,
    parts_skipped: usize,
}

impl ZoneMapScan {
    pub fn new(
        table_dir: PathBuf,
        columns: Vec<ColumnDef>,
        predicate: Predicate,
    ) -> Result<Self, ExecutionError> {
        let part_ids = discover_parts(&table_dir)?;
        Ok(Self {
            table_dir,
            columns,
            predicate,
            part_ids,
            cursor: 0,
            parts_skipped: 0,
        })
    }

    /// Parts pruned so far. For verification (see Step 5's test).
    pub fn parts_skipped(&self) -> usize {
        self.parts_skipped
    }
}

impl Processor for ZoneMapScan {
    fn next_batch(&mut self) -> Option<Result<Batch, ExecutionError>> {
        while self.cursor < self.part_ids.len() {
            let part_id = self.part_ids[self.cursor];
            self.cursor += 1;
            let part_dir = TableDef::part_dir(&self.table_dir, part_id);

            // A readable zone map may let us skip the part entirely.
            // If it's missing or unreadable, fall back to scanning (safe).
            if let Ok(zone) = read_zone_map(&part_dir) {
                if can_skip(&zone, &self.predicate) {
                    self.parts_skipped += 1;
                    if cfg!(debug_assertions) {
                        eprintln!(
                            "ZoneMapScan: skipped part {} ({} skipped)",
                            part_id, self.parts_skipped
                        );
                    }
                    continue;
                }
            }

            return Some(read_part(&part_dir, &self.columns));
        }
        None
    }
}
