use std::collections::HashMap;

use crate::aggregator::{self, Aggregator};
use crate::parser::ast::AggFunc;
use crate::storage::column_chunk::ColumnChunk;
use crate::storage::schema::{ColumnDef, DataType};

use super::batch::Batch;
use super::processor::{ExecutionError, Processor};
use super::scalar_value::{GroupKey, ScalarValue};

/// Describes one aggregate expression in the SELECT list.
pub struct AggSpec {
    pub func: AggFunc,
    /// Index of the input column in the batch from the child processor.
    pub input_col_idx: usize,
    /// Needed to instantiate fresh aggregators when a new group key appears.
    pub input_type: DataType,
    pub output_col: ColumnDef,
}

pub struct GroupByAggregate {
    input: Box<dyn Processor>,
    group_by_indices: Vec<usize>,
    group_by_schema: Vec<ColumnDef>,
    agg_specs: Vec<AggSpec>,
    group_ids: HashMap<GroupKey, u32>,
    aggs: Vec<Box<dyn Aggregator>>,
    n_groups: usize,
    done: bool,
}

impl GroupByAggregate {
    pub fn new(
        input: Box<dyn Processor>,
        group_by_indices: Vec<usize>,
        group_by_schema: Vec<ColumnDef>,
        agg_specs: Vec<AggSpec>,
    ) -> Self {
        let aggs = agg_specs
            .iter()
            .map(|spec| {
                aggregator::build(spec.func.clone(), spec.input_type.clone())
                    .expect("aggregator build failed: type should have been validated")
            })
            .collect();
        Self {
            input,
            group_by_indices,
            group_by_schema,
            agg_specs,
            group_ids: HashMap::new(),
            aggs,
            n_groups: 0,
            done: false,
        }
    }

    /// Route every row in `batch` to its group and update its aggregators.
    ///
    /// Takes fields as separate parameters because we need `groups` mutably
    /// and `agg_specs`/`group_by_indices` as shared refs simultaneously —
    /// the borrow checker cannot prove disjointness through `&mut self`.
    fn drain_batch(
        group_ids: &mut HashMap<GroupKey, u32>,
        n_groups: &mut usize,
        aggs: &mut Vec<Box<dyn Aggregator>>,
        agg_specs: &[AggSpec],
        group_by_indices: &[usize],
        batch: &Batch,
    ) -> Result<(), ExecutionError> {
        let n_rows = batch.columns[0].len();

        // Phase 1: assign a group ID to every row.
        let mut row_group_ids: Vec<u32> = Vec::with_capacity(n_rows);
        for row in 0..n_rows {
            let key: GroupKey = group_by_indices
                .iter()
                .map(|&idx| ScalarValue::from_chunk(&batch.columns[idx], row))
                .collect();
            let next_id = *n_groups as u32;
            let id = match group_ids.entry(key) {
                std::collections::hash_map::Entry::Occupied(e) => *e.get(),
                std::collections::hash_map::Entry::Vacant(e) => {
                    e.insert(next_id);
                    *n_groups += 1;
                    next_id
                }
            };
            row_group_ids.push(id);
        }

        // Phase 2: feed each aggregator the whole column + group IDs at once.
        for (agg, spec) in aggs.iter_mut().zip(agg_specs.iter()) {
            agg.update(
                &batch.columns[spec.input_col_idx],
                &row_group_ids,
                *n_groups,
            )?;
        }

        Ok(())
    }

    fn finalize_groups(
        group_by_schema: &[ColumnDef],
        agg_specs: &[AggSpec],
        group_ids: &mut HashMap<GroupKey, u32>,
        aggs: &mut Vec<Box<dyn Aggregator>>,
    ) -> Batch {
        let n_group_by = group_by_schema.len();

        // Sort keys by group ID so key columns align with aggregator output order.
        let mut keyed: Vec<(u32, GroupKey)> = group_ids.drain().map(|(k, id)| (id, k)).collect();
        keyed.sort_unstable_by_key(|(id, _)| *id);

        let mut key_cols: Vec<Vec<ScalarValue>> = (0..n_group_by)
            .map(|_| Vec::with_capacity(keyed.len()))
            .collect();
        for (_, key) in keyed {
            for (i, scalar) in key.into_iter().enumerate() {
                key_cols[i].push(scalar);
            }
        }

        let mut schema: Vec<ColumnDef> = group_by_schema.to_vec();
        schema.extend(agg_specs.iter().map(|s| s.output_col.clone()));

        let mut columns: Vec<ColumnChunk> = key_cols
            .into_iter()
            .map(ScalarValue::build_column)
            .collect();
        columns.extend(aggs.iter_mut().map(|agg| agg.finalize()));

        Batch { schema, columns }
    }
}

impl Processor for GroupByAggregate {
    fn next_batch(&mut self) -> Option<Result<Batch, ExecutionError>> {
        if self.done {
            return None;
        }

        while let Some(result) = self.input.next_batch() {
            let batch = match result {
                Ok(b) => b,
                Err(e) => return Some(Err(e)),
            };
            if let Err(e) = Self::drain_batch(
                &mut self.group_ids,
                &mut self.n_groups,
                &mut self.aggs,
                &self.agg_specs,
                &self.group_by_indices,
                &batch,
            ) {
                return Some(Err(e));
            }
        }

        self.done = true;
        Some(Ok(Self::finalize_groups(
            &self.group_by_schema,
            &self.agg_specs,
            &mut self.group_ids,
            &mut self.aggs,
        )))
    }
}
