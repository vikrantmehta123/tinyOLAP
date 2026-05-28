use std::{collections::HashMap, fmt, sync::Arc};

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
    group_values: Vec<OwnedRow>, // group_values[i] == group_index to actual group mapping.
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
            group_to_index: HashMap::new(),
            group_values: Vec::new(),
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

                // The i'th row in the batch belongs to the group -> group_indices[i]
                group_indices = Vec::with_capacity(batch.num_rows());

                for row in rows.iter() {
                    let key = row.owned();
                    let idx = match self.group_to_index.get(&key) {
                        Some(&existing) => existing,
                        None => {
                            let new_idx = self.group_values.len() as u32;

                            self.group_values.push(key.clone());
                            self.group_to_index.insert(key, new_idx);
                            new_idx
                        }
                    };

                    group_indices.push(idx);
                }

                num_groups = self.group_values.len();

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
            let owned_rows = std::mem::take(&mut self.group_values);
            match self
                .row_converter
                .convert_rows(owned_rows.iter().map(|or| or.row()))
            {
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
