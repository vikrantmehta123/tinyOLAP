//! This module converts a Physical plan into a tree of ExecutionPlan operators
//! ExecutionPlan operators actually execute the query

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use arrow::datatypes::{
    Field, Float32Type, Float64Type, Int8Type, Int16Type, Int32Type, Int64Type, Schema, UInt8Type,
    UInt16Type, UInt32Type, UInt64Type,
};

use crate::catalog::schema::{ColumnSchema, DataType, TableSchema};
use crate::config::N_WORKERS;
use crate::execution::aggregation::Accumulator;
use crate::execution::aggregation::avg::AvgAccumulator;
use crate::execution::aggregation::count::CountAccumulator;
use crate::execution::aggregation::hash_aggregate::HashAggregateExec;
use crate::execution::aggregation::max::MaxAccumulator;
use crate::execution::aggregation::merge_aggregate::MergeAggregateExec;
use crate::execution::aggregation::min::MinAccumulator;
use crate::execution::aggregation::sum::SumAccumulator;
use crate::execution::executor::{ExecutionError, ExecutionPlan};
use crate::execution::filter::FilterExec;
use crate::execution::full_scan::{FullScanExec, PartWorkSource};
use crate::execution::gather::GatherExec;
use crate::execution::limit::LimitExec;
use crate::execution::project::ProjectExec;
use crate::physical_plan::physical_operators::{AggFunc, AggSpec, PhysicalExpr, PhysicalPlan};

/// PhysicalPlan is pattern-matchable data;
/// ExecutionPlan is the runtime with state and file handles.
pub fn build(
    plan: PhysicalPlan,
    schema: &TableSchema,
    table_dir: &Path,
) -> Result<Box<dyn ExecutionPlan>, ExecutionError> {
    // For now, we assume that there is only one directory for the table
    // TODO: Later, we may want to add partitions/multiple tables.
    // Then this code will have to change
    let mut parts: Vec<PathBuf> = fs::read_dir(table_dir)?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| {
            p.is_dir()
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .map_or(false, |n| n.starts_with("part_"))
        })
        .collect();
    parts.sort();
    parts.reverse();

    let ws = Arc::new(PartWorkSource::new(parts));

    let mut children: Vec<Box<dyn ExecutionPlan>> = Vec::new();
    for _ in 0..N_WORKERS {
        let inner = build_inner(plan.clone(), schema, &ws)?;
        children.push(inner);
    }

    let gather = Box::new(GatherExec::new(N_WORKERS, children));
    let result: Box<dyn ExecutionPlan> = match find_aggregate(&plan) {
        Some((group_by, aggregates)) => {
            let accumulators = build_accumulators(aggregates, schema, false)?;
            let group_by_fields = build_group_by_fields(group_by, schema)?;
            Box::new(MergeAggregateExec::new(accumulators, gather, group_by_fields))
        }
        None => gather,
    };
    Ok(result)
}

fn find_aggregate(plan: &PhysicalPlan) -> Option<(&[PhysicalExpr], &[AggSpec])> {
    match plan {
        PhysicalPlan::Aggregate { group_by, aggregates, .. } => Some((group_by, aggregates)),
        PhysicalPlan::Filter { input, .. }
        | PhysicalPlan::Project { input, .. }
        | PhysicalPlan::Limit { input, .. } => find_aggregate(input),
        PhysicalPlan::FullScan { .. } | PhysicalPlan::ZoneMapScan { .. } => None,
    }
}

fn build_group_by_fields(
    group_by: &[PhysicalExpr],
    schema: &TableSchema,
) -> Result<Vec<Field>, ExecutionError> {
    let group_by_fields = group_by
                .iter()
                .map(|expr| match expr {
                    PhysicalExpr::Column(name) => {
                        let col_schema = schema
                            .columns
                            .iter()
                            .find(|c| c.name == *name)
                            .ok_or_else(|| {
                                ExecutionError::InvalidData(format!(
                                    "GROUP BY column '{}' not found in schema",
                                    name,
                                ))
                            })?;
                        Ok::<Field, ExecutionError>(Field::new(
                            name,
                            col_schema.data_type.to_arrow(),
                            false,
                        ))
                    }
                    _ => Err(ExecutionError::InvalidData(
                        "GROUP BY argument must be a column reference".into(),
                    )),
                })
                .collect::<Result<_, _>>()?;
    Ok(group_by_fields)
}

fn build_accumulators(
    aggregates: &[AggSpec],
    schema: &TableSchema,
    is_partial: bool,
) -> Result<Vec<Box<dyn Accumulator>>, ExecutionError> {
    // We can have more than one aggregate in the query. Hence, one accumulator per aggregation
    let mut accumulators: Vec<Box<dyn Accumulator>> = Vec::with_capacity(aggregates.len());
    for spec in aggregates {
        let column_name = match &spec.arg {
            PhysicalExpr::Column(n) => n.clone(),
            _ => {
                return Err(ExecutionError::InvalidData(
                    "Aggregate argument must be a column reference".into(),
                ));
            }
        };

        let acc: Box<dyn Accumulator> = match &spec.func {
            AggFunc::Sum => {
                let col_schema = schema
                    .columns
                    .iter()
                    .find(|c| c.name == column_name)
                    .ok_or_else(|| {
                        ExecutionError::InvalidData(format!(
                            "column '{}' not found in schema",
                            column_name,
                        ))
                    })?;

                match &col_schema.data_type {
                    DataType::I8 => Box::new(SumAccumulator::<Int8Type>::new(column_name)),
                    DataType::I16 => Box::new(SumAccumulator::<Int16Type>::new(column_name)),
                    DataType::I32 => Box::new(SumAccumulator::<Int32Type>::new(column_name)),
                    DataType::I64 => Box::new(SumAccumulator::<Int64Type>::new(column_name)),
                    DataType::U8 => Box::new(SumAccumulator::<UInt8Type>::new(column_name)),
                    DataType::U16 => Box::new(SumAccumulator::<UInt16Type>::new(column_name)),
                    DataType::U32 => Box::new(SumAccumulator::<UInt32Type>::new(column_name)),
                    DataType::U64 => Box::new(SumAccumulator::<UInt64Type>::new(column_name)),
                    DataType::F32 => Box::new(SumAccumulator::<Float32Type>::new(column_name)),
                    DataType::F64 => Box::new(SumAccumulator::<Float64Type>::new(column_name)),
                    other => {
                        return Err(ExecutionError::InvalidData(format!(
                            "SUM not supported on {:?}",
                            other,
                        )));
                    }
                }
            }
            AggFunc::Count => Box::new(CountAccumulator::new(column_name)),
            AggFunc::Min => {
                let col_schema = schema
                    .columns
                    .iter()
                    .find(|c| c.name == column_name)
                    .ok_or_else(|| {
                        ExecutionError::InvalidData(format!(
                            "column '{}' not found in schema",
                            column_name,
                        ))
                    })?;

                match &col_schema.data_type {
                    DataType::I8 => Box::new(MinAccumulator::<Int8Type>::new(column_name)),
                    DataType::I16 => Box::new(MinAccumulator::<Int16Type>::new(column_name)),
                    DataType::I32 => Box::new(MinAccumulator::<Int32Type>::new(column_name)),
                    DataType::I64 => Box::new(MinAccumulator::<Int64Type>::new(column_name)),
                    DataType::U8 => Box::new(MinAccumulator::<UInt8Type>::new(column_name)),
                    DataType::U16 => Box::new(MinAccumulator::<UInt16Type>::new(column_name)),
                    DataType::U32 => Box::new(MinAccumulator::<UInt32Type>::new(column_name)),
                    DataType::U64 => Box::new(MinAccumulator::<UInt64Type>::new(column_name)),
                    DataType::F32 => Box::new(MinAccumulator::<Float32Type>::new(column_name)),
                    DataType::F64 => Box::new(MinAccumulator::<Float64Type>::new(column_name)),
                    other => {
                        return Err(ExecutionError::InvalidData(format!(
                            "MIN not supported on {:?}",
                            other,
                        )));
                    }
                }
            }
            AggFunc::Max => {
                let col_schema = schema
                    .columns
                    .iter()
                    .find(|c| c.name == column_name)
                    .ok_or_else(|| {
                        ExecutionError::InvalidData(format!(
                            "column '{}' not found in schema",
                            column_name,
                        ))
                    })?;

                match &col_schema.data_type {
                    DataType::I8 => Box::new(MaxAccumulator::<Int8Type>::new(column_name)),
                    DataType::I16 => Box::new(MaxAccumulator::<Int16Type>::new(column_name)),
                    DataType::I32 => Box::new(MaxAccumulator::<Int32Type>::new(column_name)),
                    DataType::I64 => Box::new(MaxAccumulator::<Int64Type>::new(column_name)),
                    DataType::U8 => Box::new(MaxAccumulator::<UInt8Type>::new(column_name)),
                    DataType::U16 => Box::new(MaxAccumulator::<UInt16Type>::new(column_name)),
                    DataType::U32 => Box::new(MaxAccumulator::<UInt32Type>::new(column_name)),
                    DataType::U64 => Box::new(MaxAccumulator::<UInt64Type>::new(column_name)),
                    DataType::F32 => Box::new(MaxAccumulator::<Float32Type>::new(column_name)),
                    DataType::F64 => Box::new(MaxAccumulator::<Float64Type>::new(column_name)),
                    other => {
                        return Err(ExecutionError::InvalidData(format!(
                            "MAX not supported on {:?}",
                            other,
                        )));
                    }
                }
            }
            AggFunc::Avg => Box::new(AvgAccumulator::new(column_name, is_partial)),
        };

        accumulators.push(acc);
    }

    Ok(accumulators)
}

/// Currently, GatherExec is the root node for any plan.
/// build_inner function builds the subtree whose parent GatherExec (root) will be.
/// If we add support for JOINs, etc., then the builder has to undergo quite a few changes
/// In such cases, the optimizer has a rule that inserts Scatter/Gather operators.
/// Marking this as TODO
fn build_inner(
    plan: PhysicalPlan,
    schema: &TableSchema,
    worksource: &Arc<PartWorkSource>,
) -> Result<Box<dyn ExecutionPlan>, ExecutionError> {
    match plan {
        // Leaf: construct the scan operator directly.
        PhysicalPlan::FullScan { columns, .. } => {
            let full_scan_operator = construct_full_scan_operator(worksource, columns, schema)?;
            Ok(full_scan_operator)
        }

        // Wrapping operators: build the child first, then wrap. Same shape
        // for all three — only the wrapper type differs.
        PhysicalPlan::Filter { predicate, input } => {
            let child = build_inner(*input, schema, worksource)?;
            Ok(Box::new(FilterExec::new(predicate, child)))
        }
        PhysicalPlan::Project { projections, input } => {
            let child = build_inner(*input, schema, worksource)?;
            Ok(Box::new(ProjectExec::new(projections, child)))
        }
        PhysicalPlan::Limit { limit, input } => {
            let child = build_inner(*input, schema, worksource)?;
            Ok(Box::new(LimitExec::new(limit, child)))
        }

        // For aggregates, there is only operator even if you have multiple aggregations
        // HashAggregateExec handles multiple group bys, multiple aggregations
        PhysicalPlan::Aggregate {
            group_by,
            aggregates,
            input,
        } => {
            let accumulators = build_accumulators(&aggregates, schema, true)?;

            let child = build_inner(*input, schema, worksource)?;

            let group_by_fields: Vec<Field> = build_group_by_fields(&group_by, schema)?;
            Ok(Box::new(HashAggregateExec::new(
                accumulators,
                child,
                group_by_fields,
            )))
        }
        PhysicalPlan::ZoneMapScan { .. } => {
            unimplemented!("ZoneMapScanExec lands in TASK-004")
        }
    }
}

/// Helper function that constructs the full scan operator for the builder
fn construct_full_scan_operator(
    worksource: &Arc<PartWorkSource>,
    columns: Vec<String>,
    schema: &TableSchema,
) -> Result<Box<dyn ExecutionPlan>, ExecutionError> {
    let columns: Vec<ColumnSchema> = columns
        .iter()
        .map(|name| {
            schema
                .columns
                .iter()
                .find(|c| c.name == *name)
                .cloned()
                .ok_or_else(|| ExecutionError::InvalidData(format!("unknown column: {}", name)))
        })
        .collect::<Result<_, _>>()?;

    // Build the Arrow schema once per query. Every RecordBatch this
    // scan emits shares this exact Arc<Schema>.
    // Creating this schema in the builder means the executor doesn't have to create
    // it for every batch.
    let fields: Vec<Field> = columns
        .iter()
        .map(|c| Field::new(&c.name, c.data_type.to_arrow(), false))
        .collect();
    let arrow_schema = Arc::new(Schema::new(fields));

    Ok(Box::new(FullScanExec::new(
        worksource.clone(),
        columns,
        arrow_schema,
    )))
}
