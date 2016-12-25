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
pub mod protocol;
pub mod poll;
pub mod buf;
pub mod prop;
pub mod system;

use error::*;
pub use nix::fcntl;
pub use nix::sched;
pub use nix::sys;
pub use nix::unistd;
pub use nix::unistd::close;
use std::os::unix::io::RawFd;

#[inline]
pub fn write(fd: RawFd, buf: &[u8]) -> Result<Option<usize>> {
  let b = try!(eintr!(unistd::write, "unistd::write", fd, buf));
  Ok(b)
}

#[inline]
pub fn send(fd: RawFd, buf: &[u8], flags: sys::socket::MsgFlags) -> Result<Option<usize>> {
  let b = try!(eintr!(sys::socket::send, "sys::socket::send", fd, buf, flags));
  Ok(b)
}

#[inline]
pub fn sendto(fd: RawFd, buf: &[u8], addr: &sys::socket::SockAddr, flags: sys::socket::MsgFlags)
              -> Result<Option<usize>> {
  let b = try!(eintr!(sys::socket::sendto, "sys::socket::sendto", fd, buf, addr, flags));
  Ok(b)
}

pub fn recv(fd: RawFd, buf: &mut [u8], flags: sys::socket::MsgFlags) -> Result<Option<usize>> {

  let b = try!(eintr!(sys::socket::recv, "sys::socket::recv", fd, buf, flags));
  Ok(b)
}

#[inline]
pub fn read(fd: RawFd, buf: &mut [u8]) -> Result<Option<usize>> {
  let b = try!(eintr!(unistd::read, "unistd::read", fd, buf));
  Ok(b)
}
