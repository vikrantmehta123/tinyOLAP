// Query execution pipeline for tinyOLAP.
//
// A query is compiled into a chain of Processor nodes by build_plan, each
// pulling Batches from the node below it:
//
//   FullScan → [Filter] → [Projection | Aggregate | GroupByAggregate]
//
// Nodes are pull-based: the root calls next_batch(), which propagates down
// the chain until FullScan reads from disk.  build_plan chooses which nodes
// to wire together based on the parsed SelectStmt.


pub mod aggregate;
pub mod batch;
pub mod filter;
pub mod full_scan;
pub mod group_by_aggregate;
pub mod processor;
pub mod projection;
pub mod scalar_value;

use std::path::PathBuf;

use crate::aggregator;
use crate::parser::ast::{AggFunc, Predicate, Projection as AstProjection, SelectExpr, SelectStmt};
use crate::storage::schema::{ColumnDef, TableDef};

use self::{
    aggregate::Aggregate,
    filter::Filter,
    full_scan::FullScan,
    group_by_aggregate::{AggSpec, GroupByAggregate},
    processor::{ExecutionError, Processor},
    projection::Projection,
};

pub fn build_plan(
    table_dir: PathBuf,
    stmt: &SelectStmt,
    schema: &TableDef,
) -> Result<Box<dyn Processor>, ExecutionError> {
    let exprs = match &stmt.projection {
        AstProjection::Exprs(e) => e,
        AstProjection::All => unreachable!("Projection::All must be expanded by the analyser"),
    };

    let mut pred_names: Vec<&str> = Vec::new();
    if let Some(pred) = &stmt.where_clause {
        collect_pred_cols(pred, &mut pred_names);
    }

    let proj_names: Vec<&str> = exprs
        .iter()
        .filter_map(|e| match e {
            SelectExpr::Col(name) => Some(name.as_str()),
            SelectExpr::Agg { col, .. } if col != "*" => Some(col.as_str()),
            _ => None,
        })
        .collect();

    let group_by_names: Vec<&str> = stmt.group_by.iter().map(|s| s.as_str()).collect();

    // Column pruning: only read columns that are actually needed by this query
    let scan_cols: Vec<ColumnDef> = schema
        .columns
        .iter()
        .filter(|c| {
            proj_names.contains(&c.name.as_str())
                || pred_names.contains(&c.name.as_str())
                || group_by_names.contains(&c.name.as_str())
        })
        .cloned()
        .collect();

    let scan_cols_snapshot = scan_cols.clone();
    let mut node: Box<dyn Processor> = Box::new(FullScan::new(table_dir, scan_cols)?);

    if let Some(pred) = stmt.where_clause.clone() {
        node = Box::new(Filter::new(node, pred));
    }

    let has_agg_exprs = exprs.iter().any(|e| matches!(e, SelectExpr::Agg { .. }));
    let has_group_by = !stmt.group_by.is_empty();

    if has_agg_exprs && has_group_by {
        // Grouped aggregation: one aggregator-set per distinct key.
        let group_by_indices: Vec<usize> = stmt
            .group_by
            .iter()
            .map(|name| {
                scan_cols_snapshot
                    .iter()
                    .position(|c| &c.name == name)
                    .ok_or_else(|| {
                        ExecutionError::InvalidData(format!(
                            "GROUP BY column '{name}' not in scan (planner bug)"
                        ))
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;

        let group_by_schema: Vec<ColumnDef> = stmt
            .group_by
            .iter()
            .map(|name| {
                schema
                    .columns
                    .iter()
                    .find(|c| &c.name == name)
                    .cloned()
                    .ok_or_else(|| {
                        ExecutionError::InvalidData(format!(
                            "GROUP BY column '{name}' not in schema (planner bug)"
                        ))
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;

        let mut agg_specs: Vec<AggSpec> = Vec::new();

        for expr in exprs {
            // Bare Col exprs are the GROUP BY keys — already in group_by_schema.
            let SelectExpr::Agg { func, col } = expr else {
                continue;
            };

            // count(*) has no real input column; index 0 is a dummy since
            // CountAgg::update ignores the chunk content.
            let (input_col_idx, input_type) = if col == "*" {
                (0, scan_cols_snapshot[0].data_type.clone())
            } else {
                let col_def = schema
                    .columns
                    .iter()
                    .find(|c| &c.name == col)
                    .ok_or_else(|| {
                        ExecutionError::InvalidData(format!(
                            "aggregate refers to unknown column '{col}'"
                        ))
                    })?;
                let idx = scan_cols_snapshot
                    .iter()
                    .position(|c| c.name == *col)
                    .ok_or_else(|| {
                        ExecutionError::InvalidData(format!(
                            "aggregate column '{col}' not in scan (planner bug)"
                        ))
                    })?;
                (idx, col_def.data_type.clone())
            };

            let temp_agg = aggregator::build(func.clone(), input_type.clone())?;
            let output_type = temp_agg.output_type();
            let output_name = format!("{}({})", agg_func_name(func), col);

            agg_specs.push(AggSpec {
                func: func.clone(),
                input_col_idx,
                input_type,
                output_col: ColumnDef {
                    name: output_name,
                    data_type: output_type,
                },
            });
        }

        node = Box::new(GroupByAggregate::new(
            node,
            group_by_indices,
            group_by_schema,
            agg_specs,
        ));
    } else if has_agg_exprs {
        // Global aggregation — no GROUP BY.
        let mut aggs: Vec<Box<dyn aggregator::Aggregator>> = Vec::new();
        let mut input_idx: Vec<usize> = Vec::new();
        let mut output_schema: Vec<ColumnDef> = Vec::new();

        for expr in exprs {
            let SelectExpr::Agg { func, col } = expr else {
                return Err(ExecutionError::InvalidData(
                    "mixing column references and aggregates is not supported".into(),
                ));
            };

            let col_def = schema
                .columns
                .iter()
                .find(|c| &c.name == col)
                .ok_or_else(|| {
                    ExecutionError::InvalidData(format!(
                        "aggregate refers to unknown column '{col}'"
                    ))
                })?;
            let idx = scan_cols_snapshot
                .iter()
                .position(|c| c.name == *col)
                .ok_or_else(|| {
                    ExecutionError::InvalidData(format!(
                        "aggregate column '{col}' not in scan (planner bug)"
                    ))
                })?;

            let agg = aggregator::build(func.clone(), col_def.data_type.clone())?;
            let output_name = format!("{}({})", agg_func_name(func), col);
            let output_type = agg.output_type();

            aggs.push(agg);
            input_idx.push(idx);
            output_schema.push(ColumnDef {
                name: output_name,
                data_type: output_type,
            });
        }

        node = Box::new(Aggregate::new(node, aggs, input_idx, output_schema));
    } else {
        // Plain projection — no aggregates.
        let output_names: Vec<String> = exprs
            .iter()
            .filter_map(|e| match e {
                SelectExpr::Col(name) => Some(name.clone()),
                _ => None,
            })
            .collect();
        node = Box::new(Projection::new(node, output_names));
    }

    Ok(node)
}

fn agg_func_name(f: &AggFunc) -> &'static str {
    match f {
        AggFunc::Sum => "sum",
        AggFunc::Max => "max",
        AggFunc::Min => "min",
        AggFunc::Count => "count",
        AggFunc::Avg => "avg",
    }
}

fn collect_pred_cols<'a>(pred: &'a Predicate, out: &mut Vec<&'a str>) {
    match pred {
        Predicate::Cmp { col, .. } => out.push(col.as_str()),
        Predicate::And(a, b) | Predicate::Or(a, b) => {
            collect_pred_cols(a, out);
            collect_pred_cols(b, out);
        }
        Predicate::Not(inner) => collect_pred_cols(inner, out),
    }
}
