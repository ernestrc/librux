extern crate nix;
extern crate slab;
extern crate num_cpus;
extern crate pad;
extern crate time;
#[macro_use] extern crate lazy_static;
#[macro_use] extern crate error_chain;
#[macro_use] extern crate log;

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

pub mod constants;
pub mod handler;
pub mod protocol;
pub mod poll;
pub mod buf;
pub mod server;
pub mod logging;

use std::os::unix::io::RawFd;

pub use nix::unistd;
pub use nix::fcntl;
pub use nix::sys;

pub use nix::unistd::close;

pub fn write(fd: RawFd, buf: &[u8]) -> Result<Option<usize>> {
    let b = try!(eintr!(unistd::write, "unistd::write", fd, buf));
    Ok(b)
}

pub fn read(fd: RawFd, buf: &mut [u8]) -> Result<Option<usize>> {

    let b = try!(eintr!(unistd::read, "unistd::read", fd, buf));

    trace!("unistd::read {:?} bytes", b);

    Ok(b)
}
