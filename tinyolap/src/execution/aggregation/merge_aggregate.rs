use std::{fmt, sync::Arc};
use std::hash::BuildHasher;
use hashbrown::HashMap;
use hashbrown::hash_map::RawEntryMut;

use arrow::{
    array::{ArrayRef, RecordBatch},
    datatypes::{Field, Schema},
    row::{OwnedRow, RowConverter, SortField},
};

use crate::execution::{aggregation::Accumulator, executor::ExecutionPlan};

pub struct MergeAggregateExec {
    child: Box<dyn ExecutionPlan>,
    output_schema: Arc<Schema>,
    emitted: bool,
    accumulators: Vec<Box<dyn Accumulator>>,
    group_by_fields: Vec<Field>, // Arrow::Fields in GROUP BY clause

    row_converter: RowConverter, // We'll use Arrow's RowConverter to convert GROUP BY exprs into OwnedRow
    group_to_index: HashMap<OwnedRow, u32>, // Maps a group to its index in group_indices

    // Scratch buffer to keep the hashes; This could go in next_batch function, but having it here avoids
    // reallocation everytime we call next_batch
    hashes: Vec<u64>
}

impl MergeAggregateExec {
    /// Exactly same constructor as HashAggregateExec
    /// TODO: Check if the duplication can be eliminated
    pub fn new(
        mut accumulators: Vec<Box<dyn Accumulator>>,
        child: Box<dyn ExecutionPlan>,
        group_by_fields: Vec<Field>,
    ) -> Self {
        // If no GROUP BY clause, ensure we have one row with default output.
        if group_by_fields.is_empty() {
            for acc in accumulators.iter_mut() {
                acc.ensure_capacity(1);
            }
        }

        // RowConverter requires SortField. Create those.
        let sort_fields: Vec<SortField> = group_by_fields
            .iter()
            .map(|f| SortField::new(f.data_type().clone()))
            .collect();
        let row_converter =
            RowConverter::new(sort_fields).expect("RowConverter construction failed");

        let mut all_fields = group_by_fields.clone();
        let acc_fields: Vec<Field> = accumulators.iter().map(|a| a.output_field()).collect();
        all_fields.extend(acc_fields);
        let output_schema = Arc::new(Schema::new(all_fields));

        Self {
            child,
            accumulators,
            output_schema,
            emitted: false,
            group_by_fields,

            row_converter,
            group_to_index: HashMap::new(), // hashbrown::HashMap has a ahash hasher by default
            hashes: Vec::new(),
        }
    }
}

impl fmt::Display for MergeAggregateExec {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.fmt_indented(f, 0)
    }
}

impl ExecutionPlan for MergeAggregateExec {
    fn next_batch(
        &mut self,
    ) -> Option<Result<arrow::array::RecordBatch, crate::execution::executor::ExecutionError>> {
        if self.emitted {
            return None;
        }

        loop {
            let batch = match self.child.next_batch() {
                None => break,
                Some(Ok(b)) => b,
                Some(Err(e)) => return Some(Err(e)),
            };

            let mut group_indices: Vec<u32>;
            let num_groups;

            if self.group_by_fields.is_empty() {
                group_indices = vec![0u32; batch.num_rows()];
                num_groups = 1;
            }
            else{
                let group_by_arrays: Vec<ArrayRef> = self
                    .group_by_fields
                    .iter()
                    .map(|f| {
                        batch
                            .column_by_name(f.name())
                            .expect("group-by column missing from batch — planner bug")
                            .clone()
                    })
                    .collect();

                let rows = match self.row_converter.convert_columns(&group_by_arrays) {
                    Ok(r) => r,
                    Err(e) => return Some(Err(e.into())),
                };

                // Pass 1: Pre-hash everything in a tight loop
                // Clear the hashes vector and compute hashes
                // Due to the tight for loop, compiler can vectorize this
                self.hashes.clear();
                self.hashes.reserve(batch.num_rows());

                let hasher = self.group_to_index.hasher();

                for row in rows.iter() {
                    self.hashes.push(hasher.hash_one(row.as_ref()));
                }

                // Pass 2: Probe with pre-computed hashes
                group_indices = Vec::with_capacity(batch.num_rows());

                for (i, row) in rows.iter().enumerate() {
                    let hash = self.hashes[i];
                    let bytes = row.as_ref();

                    let next_idx = self.group_to_index.len() as u32;

                    let idx = match self.group_to_index
                        .raw_entry_mut()
                        .from_hash(hash, |existing_key: &OwnedRow| {existing_key.as_ref() == bytes})
                    {
                        RawEntryMut::Occupied(e) => *e.get(),
                        RawEntryMut::Vacant(e) => {
                            e.insert_hashed_nocheck(hash, row.owned(), next_idx);
                            next_idx
                        }
                    };
                    group_indices.push(idx);
                }
                num_groups = self.group_to_index.len();

            }

            for acc in self.accumulators.iter_mut() {
                if let Err(e) = acc.merge(&batch, &group_indices, num_groups) {
                    return Some(Err(e));
                }
            }
        }

        // Drain finished — Build the output as: [Group By column, Accumulator columns]
        let group_arrays: Vec<ArrayRef> = if self.group_by_fields.is_empty() {
            vec![]
        } else {
            let n = self.group_to_index.len();
            let mut ordered: Vec<Option<&OwnedRow>> = vec![None; n];
            for (k, &v) in self.group_to_index.iter() {
                ordered[v as usize] = Some(k);
            }
            let rows_iter = ordered
                .iter()
                .map(|opt| opt.expect("group index gap — bug").row());
            match self.row_converter.convert_rows(rows_iter) {
                Ok(arrs) => arrs,
                Err(e) => return Some(Err(e.into())),
            }
        };


        let acc_arrays: Vec<ArrayRef> = self
            .accumulators
            .iter_mut()
            .map(|acc| acc.materialize())
            .collect();

        let mut all_arrays = group_arrays;
        all_arrays.extend(acc_arrays);

        let batch = match RecordBatch::try_new(self.output_schema.clone(), all_arrays) {
            Ok(b) => b,
            Err(e) => return Some(Err(e.into())),
        };

        self.emitted = true;
        Some(Ok(batch))
    }

    fn fmt_indented(&self, f: &mut fmt::Formatter<'_>, depth: usize) -> fmt::Result {
        let indent = "  ".repeat(depth);

        let group_by_names: Vec<String> = self
            .group_by_fields
            .iter()
            .map(|f| f.name().to_string())
            .collect();

        let agg_names: Vec<String> = self
            .accumulators
            .iter()
            .map(|a| a.output_field().name().to_string())
            .collect();

        writeln!(
            f,
            "{}MergeAggregateExec(group_by=[{}], aggregates=[{}])",
            indent,
            group_by_names.join(", "),
            agg_names.join(", "),
        )?;
        self.child.fmt_indented(f, depth + 1)
    }
}
