//! Logical Plan's Optimizer Rules
//! 
//! This module defines the rules that the optimizer applies to
//! optimize the logical plan

pub mod type_coercion;
pub mod constant_folding;
pub mod eliminate_true_filter;