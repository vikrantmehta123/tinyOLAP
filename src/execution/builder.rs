use std::path::Path;

use crate::catalog::schema::TableSchema;
use crate::execution::executor::{ExecutionError, ExecutionPlan};
use crate::execution::filter::FilterExec;
use crate::execution::full_scan::FullScanExec;
use crate::execution::limit::LimitExec;
use crate::execution::project::ProjectExec;
use crate::physical_plan::physical_operators::PhysicalPlan;

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

        // Out of scope for TASK-002.
        PhysicalPlan::Aggregate { .. } => {
            unimplemented!("HashAggregateExec lands in TASK-003")
        }
        PhysicalPlan::ZoneMapScan { .. } => {
            unimplemented!("ZoneMapScanExec lands in TASK-004")
        }
    }
}
