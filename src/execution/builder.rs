use std::path::Path;

use arrow::datatypes::{Int8Type, Int16Type, Int32Type, Int64Type, UInt8Type, UInt16Type, UInt32Type, UInt64Type, Float32Type, Float64Type};

use crate::catalog::schema::{TableSchema, DataType};
use crate::execution::aggregation::sum::SumExec;
use crate::execution::executor::{ExecutionError, ExecutionPlan};
use crate::execution::filter::FilterExec;
use crate::execution::full_scan::FullScanExec;
use crate::execution::limit::LimitExec;
use crate::execution::project::ProjectExec;
use crate::physical_plan::physical_operators::{AggFunc, PhysicalExpr, PhysicalPlan};

/// Turns a PhysicalPlan (intent, post-optimizer) into a tree of running
/// ExecutionPlan operators. PhysicalPlan is pattern-matchable data;
/// ExecutionPlan is the runtime with state and file handles. Keeping them
/// separate means the optimizer never thinks about runtime, and operators
/// never think about rewrites.
pub fn build(
    plan: PhysicalPlan,
    schema: &TableSchema,
    table_dir: &Path,
) -> Result<Box<dyn ExecutionPlan>, ExecutionError> {
    match plan {
        // Leaf: construct the scan operator directly.
        PhysicalPlan::FullScan { columns, .. } => {
            let exec = FullScanExec::new(table_dir, columns, schema)?;
            Ok(Box::new(exec))
        }

        // Wrapping operators: build the child first, then wrap. Same shape
        // for all three — only the wrapper type differs.
        PhysicalPlan::Filter { predicate, input } => {
            let child = build(*input, schema, table_dir)?;
            Ok(Box::new(FilterExec::new(predicate, child)))
        }
        PhysicalPlan::Project { projections, input } => {
            let child = build(*input, schema, table_dir)?;
            Ok(Box::new(ProjectExec::new(projections, child)))
        }
        PhysicalPlan::Limit { limit, input } => {
            let child = build(*input, schema, table_dir)?;
            Ok(Box::new(LimitExec::new(limit, child)))
        }

        PhysicalPlan::Aggregate {
            group_by,
            aggregates,
            input,
        } => {
            // Right now, we don't support GROUP BY. Only sum is present
            // Validate that
            if !group_by.is_empty()
                || aggregates.len() != 1
                || !matches!(aggregates[0].func, AggFunc::Sum)
            {
                return Err(ExecutionError::InvalidData(
                    "Only SUM with no GROUP BY is supported".into(),
                ));
            }

            let child = build(*input, schema, table_dir)?;
            let column_name = match &aggregates[0].arg {
                PhysicalExpr::Column(n) => n.clone(),
                _ => {
                    return Err(ExecutionError::InvalidData(
                        "SUM argument must be a column reference".into(),
                    ));
                }
            };

            // Look up the column in the schema to find its data type.
            let col_schema = schema.columns.iter()
                .find(|c| c.name == column_name)
                .ok_or_else(|| ExecutionError::InvalidData(format!(
                    "column '{}' not found in schema", column_name,
                )))?;

            let exec: Box<dyn ExecutionPlan> = match &col_schema.data_type {
                DataType::I8   => Box::new(SumExec::<Int8Type>  ::new(column_name, child)),
                DataType::I16  => Box::new(SumExec::<Int16Type> ::new(column_name, child)),
                DataType::I32  => Box::new(SumExec::<Int32Type> ::new(column_name, child)),
                DataType::I64  => Box::new(SumExec::<Int64Type> ::new(column_name, child)),
                DataType::U8   => Box::new(SumExec::<UInt8Type> ::new(column_name, child)),
                DataType::U16  => Box::new(SumExec::<UInt16Type>::new(column_name, child)),
                DataType::U32  => Box::new(SumExec::<UInt32Type>::new(column_name, child)),
                DataType::U64  => Box::new(SumExec::<UInt64Type>::new(column_name, child)),
                DataType::F32  => Box::new(SumExec::<Float32Type>::new(column_name, child)),
                DataType::F64  => Box::new(SumExec::<Float64Type>::new(column_name, child)),
                other => return Err(ExecutionError::InvalidData(format!(
                    "SUM not supported on {:?}", other,
                ))),
            };
            Ok(exec)


        }
        PhysicalPlan::ZoneMapScan { .. } => {
            unimplemented!("ZoneMapScanExec lands in TASK-004")
        }
    }
}
