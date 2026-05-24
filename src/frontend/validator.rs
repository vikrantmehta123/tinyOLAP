//! SQL Shape validation — rejects SQL structures that tinyOLAP does not support.
//! Runs before the analyzer; it only checks the structure of the AST.


use sqlparser::ast::{
    Expr, FunctionArguments, GroupByExpr, SelectItem, 
    SetExpr, Statement, TableFactor
};

pub fn validate(stmt: &Statement) -> Result<(), String> {
    match stmt {
        Statement::Insert(_) => Ok(()), 
        Statement::Query(query) => validate_query(query), 
        _ => Err("Unsupported Statement type".to_string())
    }
}

fn validate_query(query: &sqlparser::ast::Query)-> Result<(), String> {
    if query.with.is_some() {
        return Err("WITH / CTEs are not supported".to_string())
    }

    if query.fetch.is_some() {
        return Err("FETCH is not supported".to_string())
    }

    match query.body.as_ref() {
        SetExpr::Select(select) => validate_select(select), 
        _ => Err("UNION/ INTERSECT/ EXCEPT/ FOR clauses are not supported".to_string())
    }

    // TODO: Implement a validation for ORDER BY clause once that is added in support

}

fn validate_select(select: &sqlparser::ast::Select) -> Result<(), String> {
    if select.distinct.is_some() {
        return Err("DISTINCT is not supported".to_string())
    }

    
    // TODO: Implement a validation for HAVING clause when that clause is supported


    if select.from.len() != 1 {
        return Err("Exactly one table in FROM is required".to_string())
    }

    let from = &select.from[0];

    // JOINs appear as entries on the first FROM item, not as extra FROM items.
    if !from.joins.is_empty() {
        return Err("JOINs are not supported".to_string());
    }

    match &from.relation {
        TableFactor::Table {..} => {},
        _ => return Err("Subqueries in FROM are not supported".to_string())
    }

    for item in &select.projection {
        validate_select_item(item)?;
    }

    if let Some(expr) = &select.selection {
        validate_where_expr(expr)?;
    }

    // TODO: Validate whether all non-aggregate columns are in GROUP By clause or not.
    match &select.group_by {
        GroupByExpr::Expressions(exprs, _) => {
            for e in exprs {
                match e {
                    Expr::Identifier(_) | Expr::CompoundIdentifier(_) => {},
                    _ => return Err("only column names are supported in GROUP BY".to_string()),
                }
            }
        }, 
        _ => return Err("ROLLUP / CUBE in GROUP BY is not supported".to_string())
    }


    Ok(())
}


fn validate_select_item(item: &SelectItem) -> Result<(), String> {
    match item  {
        SelectItem::Wildcard(_) => Ok(()), 
        SelectItem::UnnamedExpr(expr) => validate_projection_expr(expr), 
        SelectItem::ExprWithAlias { expr, .. } => validate_projection_expr(expr), 
        _ => Err("Unsupported SELECT item".to_string())
    }
}

fn validate_where_expr(expr: &Expr) -> Result<(), String> {
    match expr {
        Expr::BinaryOp { left , op, right } => {
            use sqlparser::ast::BinaryOperator::*;

            match op {
                Eq | NotEq | Lt | Gt | LtEq | GtEq | And | Or => {}
                _ => return Err(format!("Unsupported operator in WHERE: {:?}", op)),
            }
            // Recurse into both sides so nested bad operators/subqueries are caught too.
            validate_where_expr(left)?;
            validate_where_expr(right)?;
            Ok(())
        }, 
        Expr::Identifier(_) | Expr::CompoundIdentifier(_) => Ok(()),
        Expr::Value(_) => Ok(()),
        Expr::Function(_) => Err("functions in WHERE are not supported".to_string()),
        Expr::Subquery(_) => Err("subqueries in WHERE are not supported".to_string()),
        _ => Err(format!("Unsupported expressed in WHERE: {:?}", expr)),
    }
}

fn validate_projection_expr(expr: &Expr) -> Result<(), String> {
    match expr {
        Expr::Identifier(_) | Expr::CompoundIdentifier(_) => Ok(()), 
        Expr::Function(f) => {
            let name = f.name.to_string().to_uppercase();
            if !["COUNT", "SUM", "AVG", "MIN", "MAX"].contains(&name.as_str()) {
                return Err(format!("Unsupported aggregate function: {}", name));
            }
            match &f.args {
                FunctionArguments::List(arg_list) => {
                    if arg_list.args.len() != 1 {
                        return Err("aggregate functions take exactly one argument".to_string());
                    }
                }, 
                _ => {
                    return Err("Unsupported function argument for the function".to_string());
                }
            }
            Ok(())
        }
        _ => Err("Unsupported expression in SELECT list".to_string())
    }    
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::frontend::parser::parse;

    fn validate_sql(sql: &str) -> Result<(), String> {
        let stmt = parse(sql).unwrap();
        validate(&stmt)
    }

    #[test]
    fn valid_select_passes() {
        assert!(validate_sql("SELECT id, name FROM users").is_ok());
    }

    #[test]
    fn select_star_passes() {
        assert!(validate_sql("SELECT * FROM users").is_ok());
    }

    #[test]
    fn group_by_passes() {
        assert!(validate_sql("SELECT country, COUNT(id) FROM users GROUP BY country").is_ok());
    }

    #[test]
    fn limit_passes() {
        assert!(validate_sql("SELECT id FROM users LIMIT 10").is_ok());        
    }

    #[test]
    fn rejects_cte() {
        assert!(validate_sql("WITH cte AS (SELECT 1) SELECT * FROM cte").is_err());
    }

    #[test]
    fn rejects_join() {
        assert!(validate_sql("SELECT u.id FROM users u JOIN orders o ON u.id = o.user_id").is_err());
    }

    #[test]
    fn rejects_subquery_in_where() {
        assert!(validate_sql("SELECT id FROM users WHERE id = (SELECT MAX(id) FROM users)").is_err());
    }

    #[test]
    fn rejects_distinct() {
        assert!(validate_sql("SELECT DISTINCT id FROM users").is_err());
    }

    #[test]
    fn rejects_unknown_aggregate_function() {
        assert!(validate_sql("SELECT id FROM users WHERE ABS(age) > 10").is_err());
    }

    #[test]
    fn rejects_unsupported_operator_in_where() {
        assert!(validate_sql("select id from users where name like '%alice%'").is_err());
    }

}