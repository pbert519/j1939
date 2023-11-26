#![cfg_attr(not(any(feature = "std", test)), no_std)]
#![warn(missing_docs)]

//! A J1939 Stack
//! Uses a [`embedded_can::blocking::Can`] can driver

extern crate alloc;

/// Control Function
pub mod control_function;
/// J1939 Frames
pub mod frame;
/// J1939 Name and enums
pub mod name;
/// J1939 Stack
pub mod stack;
/// Time utilities for the stack
pub mod time;

mod address;
#[cfg(test)]
mod test_utils;
mod transport;
