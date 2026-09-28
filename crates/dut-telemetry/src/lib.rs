//! Console logging, and the log-field conventions every crate in the
//! workspace follows.

mod console;
mod fields;
mod request_blocks;

pub use console::init;
pub use fields::millis;
pub use request_blocks::REQUEST_SPAN;
