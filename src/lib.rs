extern crate nix;
extern crate slab;
extern crate num_cpus;
extern crate pad;
extern crate time;
#[macro_use] extern crate lazy_static;
#[macro_use] extern crate error_chain;
#[macro_use] extern crate log;

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
                Err(::nix::Error::Sys(::nix::errno::EAGAIN)) => {
                    trace!("{}: EAGAIN: socket not ready", $name);
                    res = Ok(None);
                    break;
                },
                Err(::nix::Error::Sys(::nix::errno::EINTR)) => {
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

mod constants;
pub mod protocol;
pub mod poll;
pub mod buf;
pub mod handler;
pub mod server;
pub mod logging;
pub mod error;

use nix::unistd;

pub use poll::{Epoll, EpollFd, EpollEvent};
pub use handler::Handler;
pub use handler::sync::SyncHandler;
pub use server::{Server, ServerImpl};
pub use logging::{LoggingBackend, SimpleLogging};
pub use error::Result;
pub use protocol::{IOProtocol, Action};

pub use std::os::unix::io::{AsRawFd, RawFd};
pub use nix::unistd::close;
pub use nix::fcntl;
pub use nix::sys::stat;

pub fn write(fd: RawFd, buf: &[u8]) -> Result<Option<usize>> {
    let b = try!(eintr!(unistd::write, "unistd::write", fd, buf));
    Ok(b)
}

pub fn read(fd: RawFd, buf: &mut [u8]) -> Result<Option<usize>> {

    let b = try!(eintr!(unistd::read, "unistd::read", fd, buf));

    trace!("unistd::read {:?} bytes", b);

    Ok(b)
}
