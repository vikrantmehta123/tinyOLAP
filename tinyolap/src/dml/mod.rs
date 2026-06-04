//! Data Manipulation Language Module
//! 
//! This module handles the parsing and planning for DML queries
//! like INSERTs. For the moment, only INSERT queries are supported.
//! TODO: We plan to add CREATE TABLE, etc. statements. They will go in
//! this module.

pub mod insert_builder;