//! Physical Planning Module for TinyOLAP
//!
//! Defines physical plan operators, optimizations on the physical
//! plans and the lowering function from Logical Plan to Physical.

pub mod lower;
pub mod optimizer;
pub mod physical_operators;
