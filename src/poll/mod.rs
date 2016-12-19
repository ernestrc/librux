

use error::Result;
use handler::Handler;

pub use nix::sys::epoll::{epoll_create, EpollEvent, EpollEventKind, EPOLLIN, EPOLLOUT, EPOLLERR, EPOLLHUP, EPOLLET,
                          EPOLLONESHOT, EPOLLRDHUP, EPOLLEXCLUSIVE, EPOLLWAKEUP};

use nix::sys::epoll::{epoll_ctl, epoll_wait, EpollOp};
use nix::unistd;
use std::fmt;
use std::os::unix::io::RawFd;

lazy_static! {
    static ref NO_INTEREST: EpollEvent = {
        EpollEvent {
            events: EpollEventKind::empty(),
            data: 0,
        }
    };
}

#[derive(Debug, Copy, Clone)]
pub struct EpollConfig {
    pub loop_ms: isize,
    pub buffer_capacity: usize,
}

pub struct Epoll<H> {
    pub epfd: EpollFd,
    handler: H,
    loop_ms: isize,
    buf: Vec<EpollEvent>,
}

#[derive(Debug, Copy, Clone)]
pub struct EpollFd {
    pub fd: RawFd,
}

pub enum EpollCmd {
    Shutdown,
    Poll,
}

unsafe impl<H> Send for Epoll<H> {}

impl<H: Handler<EpollEvent, EpollCmd>> Epoll<H> {
    pub fn from_fd(epfd: EpollFd, handler: H, config: EpollConfig) -> Epoll<H> {
        Epoll {
            epfd: epfd,
            loop_ms: config.loop_ms,
            handler: handler,
            buf: Vec::with_capacity(config.buffer_capacity),
        }
    }

    pub fn new_with<F>(config: EpollConfig, newctl: F) -> Result<Epoll<H>>
        where F: FnOnce(EpollFd) -> H,
    {

        let fd = try!(epoll_create());

        let epfd = EpollFd {
            fd: fd,
        };

        let handler = newctl(epfd);

        Ok(Self::from_fd(epfd, handler, config))
    }

    #[inline(always)]
    pub fn run_once(&mut self) {
        let dst = unsafe { ::std::slice::from_raw_parts_mut(self.buf.as_mut_ptr(), self.buf.capacity()) };
        let cnt = epoll_wait(self.epfd.fd, dst, self.loop_ms).unwrap();
        unsafe { self.buf.set_len(cnt) }

        {
            for ev in self.buf.iter() {
                if let EpollCmd::Shutdown = self.handler.ready(*ev) {
                    return;
                }
            }
        }
    }

    pub fn run(&mut self) {
        loop {
            self.run_once();
        }
    }
}

//impl_fn_handler!(EpollEvent, EpollCmd);

impl<H> Drop for Epoll<H> {
    fn drop(&mut self) {
        let _ = unistd::close(self.epfd.fd);
    }
}

unsafe impl Send for EpollFd {}

impl EpollFd {
    pub fn new(fd: RawFd) -> EpollFd {
        EpollFd {
            fd: fd,
        }
    }

    #[inline(always)]
    fn ctl(&self, op: EpollOp, interest: &EpollEvent, fd: RawFd) -> Result<()> {
        try!(epoll_ctl(self.fd, op, fd, interest));
        Ok(())
    }

    #[inline(always)]
    pub fn reregister(&self, fd: RawFd, interest: &EpollEvent) -> Result<()> {
        trace!("reregister()");
        try!(self.ctl(EpollOp::EpollCtlMod, interest, fd));
        Ok(())
    }

    #[inline(always)]
    pub fn register(&self, fd: RawFd, interest: &EpollEvent) -> Result<()> {
        trace!("register()");
        try!(self.ctl(EpollOp::EpollCtlAdd, interest, fd));
        Ok(())
    }

    #[inline(always)]
    pub fn unregister(&self, fd: RawFd) -> Result<()> {
        trace!("unregister()");
        try!(self.ctl(EpollOp::EpollCtlDel, &NO_INTEREST, fd));
        Ok(())
    }
}

impl fmt::Display for EpollFd {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        write!(fmt, "{}", self.fd)
    }
}

impl From<EpollFd> for i32 {
    fn from(epfd: EpollFd) -> i32 {
        epfd.fd
    }
}

impl Default for EpollConfig {
    fn default() -> EpollConfig {
        EpollConfig {
            loop_ms: -1,
            buffer_capacity: 256,
        }
    }
}


#[cfg(test)]
mod tests {
    use handler::Handler;
    use nix::fcntl::O_NONBLOCK;
    use nix::unistd;
    use ::std::sync::mpsc::*;
    use super::*;

    struct ChannelHandler {
        tx: Sender<EpollEvent>,
    }

    impl Handler<EpollEvent, EpollCmd> for ChannelHandler {

        fn ready(&mut self, events: EpollEvent) -> EpollCmd {
            if self.tx.send(events).is_ok() {
                return EpollCmd::Shutdown;
            }

            EpollCmd::Poll
        }
    }

    #[test]
    fn notify_handler() {

        let (tx, rx) = channel();

        let config = EpollConfig {
            loop_ms: 10,
            buffer_capacity: 100,
        };

        let mut poll = Epoll::new_with(config, |_| {
                ChannelHandler {
                    tx: tx,
                }
            })
            .unwrap();

        let (rfd, wfd) = unistd::pipe2(O_NONBLOCK).unwrap();

        let interest = EpollEvent {
            events: EPOLLONESHOT | EPOLLIN,
            data: rfd as u64,
        };

        unistd::write(wfd, b"hello!").unwrap();

        poll.epfd.register(rfd, &interest).unwrap();

        poll.run_once();

        let ev = rx.recv().unwrap();

        assert!(ev.events.contains(EPOLLIN));
        assert!(ev.data == rfd as u64);
    }
}
