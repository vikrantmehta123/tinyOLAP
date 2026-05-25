use std::path::Path;

use arrow::datatypes::{Field, Float32Type, Float64Type, Int8Type, Int16Type, Int32Type, Int64Type, UInt8Type, UInt16Type, UInt32Type, UInt64Type};

use crate::catalog::schema::{TableSchema, DataType};
use crate::execution::aggregation::Accumulator;
use crate::execution::aggregation::count::CountAccumulator;
use crate::execution::aggregation::hash_aggregate::HashAggregateExec;
use crate::execution::aggregation::max::MaxAccumulator;
use crate::execution::aggregation::min::MinAccumulator;
use crate::execution::aggregation::sum::SumAccumulator;
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
            
            let mut accumulators: Vec<Box<dyn Accumulator>> = Vec::with_capacity(aggregates.len());
            for spec in &aggregates {
                let column_name = match &spec.arg {
                    PhysicalExpr::Column(n) => n.clone(), 
                    _ => return Err(ExecutionError::InvalidData("Aggregate argument must be a column reference".into()))
                };

                let acc: Box<dyn Accumulator> = match &spec.func {
                    AggFunc::Sum => {
                        let col_schema = schema.columns.iter()
                        .find(|c| c.name == column_name)
                        .ok_or_else(|| ExecutionError::InvalidData(format!(
                            "column '{}' not found in schema", column_name,
                        )))?;

                        match &col_schema.data_type {
                            DataType::I8   => Box::new(SumAccumulator::<Int8Type>  ::new(column_name)),
                            DataType::I16  => Box::new(SumAccumulator::<Int16Type> ::new(column_name)),
                            DataType::I32  => Box::new(SumAccumulator::<Int32Type> ::new(column_name)),
                            DataType::I64  => Box::new(SumAccumulator::<Int64Type> ::new(column_name)),
                            DataType::U8   => Box::new(SumAccumulator::<UInt8Type> ::new(column_name)),
                            DataType::U16  => Box::new(SumAccumulator::<UInt16Type>::new(column_name)),
                            DataType::U32  => Box::new(SumAccumulator::<UInt32Type>::new(column_name)),
                            DataType::U64  => Box::new(SumAccumulator::<UInt64Type>::new(column_name)),
                            DataType::F32  => Box::new(SumAccumulator::<Float32Type>::new(column_name)),
                            DataType::F64  => Box::new(SumAccumulator::<Float64Type>::new(column_name)),
                            other => return Err(ExecutionError::InvalidData(format!(
                                "SUM not supported on {:?}", other,
                            ))),
                        }
                    }, 
                    AggFunc::Count => {
                        Box::new(CountAccumulator::new(column_name))
                    }, 
                    AggFunc::Min => {
                        let col_schema = schema.columns.iter()
                            .find(|c| c.name == column_name)
                            .ok_or_else(|| ExecutionError::InvalidData(format!(
                                "column '{}' not found in schema", column_name,
                            )))?;

                        match &col_schema.data_type {
                            DataType::I8   => Box::new(MinAccumulator::<Int8Type>  ::new(column_name)),
                            DataType::I16  => Box::new(MinAccumulator::<Int16Type> ::new(column_name)),
                            DataType::I32  => Box::new(MinAccumulator::<Int32Type> ::new(column_name)),
                            DataType::I64  => Box::new(MinAccumulator::<Int64Type> ::new(column_name)),
                            DataType::U8   => Box::new(MinAccumulator::<UInt8Type> ::new(column_name)),
                            DataType::U16  => Box::new(MinAccumulator::<UInt16Type>::new(column_name)),
                            DataType::U32  => Box::new(MinAccumulator::<UInt32Type>::new(column_name)),
                            DataType::U64  => Box::new(MinAccumulator::<UInt64Type>::new(column_name)),
                            DataType::F32  => Box::new(MinAccumulator::<Float32Type>::new(column_name)),
                            DataType::F64  => Box::new(MinAccumulator::<Float64Type>::new(column_name)),
                            other => return Err(ExecutionError::InvalidData(format!(
                                "MIN not supported on {:?}", other,
                            ))),
                        }
                    },
                    AggFunc::Max => {
                        let col_schema = schema.columns.iter()
                            .find(|c| c.name == column_name)
                            .ok_or_else(|| ExecutionError::InvalidData(format!(
                                "column '{}' not found in schema", column_name,
                            )))?;

                        match &col_schema.data_type {
                            DataType::I8   => Box::new(MaxAccumulator::<Int8Type>  ::new(column_name)),
                            DataType::I16  => Box::new(MaxAccumulator::<Int16Type> ::new(column_name)),
                            DataType::I32  => Box::new(MaxAccumulator::<Int32Type> ::new(column_name)),
                            DataType::I64  => Box::new(MaxAccumulator::<Int64Type> ::new(column_name)),
                            DataType::U8   => Box::new(MaxAccumulator::<UInt8Type> ::new(column_name)),
                            DataType::U16  => Box::new(MaxAccumulator::<UInt16Type>::new(column_name)),
                            DataType::U32  => Box::new(MaxAccumulator::<UInt32Type>::new(column_name)),
                            DataType::U64  => Box::new(MaxAccumulator::<UInt64Type>::new(column_name)),
                            DataType::F32  => Box::new(MaxAccumulator::<Float32Type>::new(column_name)),
                            DataType::F64  => Box::new(MaxAccumulator::<Float64Type>::new(column_name)),
                            other => return Err(ExecutionError::InvalidData(format!(
                                "MAX not supported on {:?}", other,
                            ))),
                        }
                    },

                    other => return Err(ExecutionError::InvalidData(format!(
                        "aggregate function {:?} not supported yet", other,
                    ))),
                };

                accumulators.push(acc);

            }

            let child = build(*input, schema, table_dir)?;

            let group_by_fields: Vec<Field> = group_by.iter().map(|expr| match expr {
                PhysicalExpr::Column(name) => {
                    let col_schema = schema.columns.iter()
                        .find(|c| c.name == *name)
                        .ok_or_else(|| ExecutionError::InvalidData(format!(
                            "GROUP BY column '{}' not found in schema", name,
                        )))?;
                    Ok::<Field, ExecutionError>(Field::new(name, col_schema.data_type.to_arrow(), false))
                }
                _ => Err(ExecutionError::InvalidData(
                    "GROUP BY argument must be a column reference".into(),
                )),
            }).collect::<Result<_, _>>()?;


            Ok(Box::new(HashAggregateExec::new(accumulators, child, group_by_fields)))
        }
        PhysicalPlan::ZoneMapScan { .. } => {
            unimplemented!("ZoneMapScanExec lands in TASK-004")
        }
    }
}
