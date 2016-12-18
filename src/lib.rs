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

use error::*;

#[macro_export]
macro_rules! eintr {
    ($syscall:expr, $name:expr, $($arg:expr),*) => {{
        let res;
        loop {
            match $syscall($($arg),*) {
                Ok(m) => {
                    res = Ok(Some(m));
                    break;
                },
                Err(SysError(EAGAIN)) => {
                    trace!("{}: EAGAIN: socket not ready", $name);
                    res = Ok(None);
                    break;
                },
                Err(SysError(EINTR)) => {
                    debug!("{}: EINTR: re-submitting syscall", $name);
                    continue;
                },
                Err(err) => {
                    res = Err(err);
                    break;
                }
            }
        }
        res
    }}
}

#[macro_export]
macro_rules! report_err {
    ($name:expr, $err:expr) => {{
        let e: Error = $err;
        error!("{}: {}\n{:?}", $name, e, e.backtrace());

        for e in e.iter().skip(1) {
            error!("caused_by: {}", e);
        }
    }}
}

/// helper to short-circuit loops and log error
#[macro_export]
macro_rules! perrorr {
    ($name:expr, $res:expr) => {{
        match $res {
            Ok(s) => s,
            Err(err) => {
                let err: Error = err.into();
                report_err!($name, err);
                return;
            }
        }
    }}
}

#[macro_export]
macro_rules! perror {
    ($name:expr, $res:expr) => {{
        match $res {
            Ok(s) => s,
            Err(e) => {
                report_err!($name, e.into());
            },
        }
    }}
}

pub mod handler;
pub mod protocol;
pub mod poll;
pub mod buf;
pub mod prop;

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
pub fn sendto(
    fd: RawFd,
    buf: &[u8],
    addr: &sys::socket::SockAddr,
    flags: sys::socket::MsgFlags
) -> Result<Option<usize>> {
    let b = try!(eintr!(sys::socket::sendto,
                        "sys::socket::sendto",
                        fd,
                        buf,
                        addr,
                        flags));
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
