//! Tengo: a small, dynamic, fast scripting language, ported from Go to Rust.

pub mod ast;
pub mod builtins;
pub mod cli;
pub mod compiler;
pub mod formatter;
pub mod ftoa;
pub mod gomath;
pub mod gomathfn;
pub mod goregex;
pub mod goslice;
pub mod gostr;
pub mod gotables;
pub mod gotime;
pub mod json;
pub mod objects;
pub mod out;
pub mod parser;
pub mod scanner;
pub mod source;
pub mod stdlib;
pub mod text;
pub mod times;
pub mod token;
pub mod vm;
