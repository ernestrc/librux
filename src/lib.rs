extern crate nix;
extern crate slab;
extern crate num_cpus;
extern crate pad;
extern crate time;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate error_chain;
#[macro_use]
extern crate log;

pub mod error;
#[macro_use]
pub mod macros;
pub mod handler;
pub mod mux;
pub mod epoll;
pub mod buf;
pub mod prop;
pub mod daemon;

pub use nix::*;
pub use std::os::unix::io::RawFd;

pub trait Reset {
  fn reset(&mut self);
}
