//! Shim: exposes the real graphics submodule files to the showcase2
//! generator. (A `#[path]` include of `rust/src/graphics.rs` would make
//! its child modules resolve relative to this generator's directory, so
//! each child is pathed explicitly here instead.)

#[path = "../../../rust/src/graphics/command.rs"]
pub mod command;
#[path = "../../../rust/src/graphics/combine.rs"]
pub mod combine;
#[path = "../../../rust/src/graphics/frame.rs"]
pub mod frame;
#[path = "../../../rust/src/graphics/gx_backend.rs"]
pub mod gx_backend;
#[path = "../../../rust/src/graphics/interpreter.rs"]
pub mod interpreter;
#[path = "../../../rust/src/graphics/segments.rs"]
pub mod segments;
#[path = "../../../rust/src/graphics/state.rs"]
pub mod state;
#[path = "../../../rust/src/graphics/texture.rs"]
pub mod texture;
#[path = "../../../rust/src/graphics/vertex.rs"]
pub mod vertex;
