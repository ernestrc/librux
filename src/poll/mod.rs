use std::os::unix::io::RawFd;
use std::{slice, fmt};

use nix::sys::epoll::{epoll_ctl, epoll_wait, EpollOp};
use nix::unistd;

use error::{Result, Error};
use handler::Handler;

pub use nix::sys::epoll::{epoll_create, EpollEvent, EpollEventKind, EPOLLIN, EPOLLOUT, EPOLLERR,
                          EPOLLHUP, EPOLLET, EPOLLONESHOT, EPOLLRDHUP, EPOLLEXCLUSIVE, EPOLLWAKEUP};

static EVENTS_N: &'static usize = &1000;

lazy_static! {
    static ref NO_INTEREST: EpollEvent = {
        EpollEvent {
            events: EpollEventKind::empty(),
            data: 0,
        }
    };
}

pub struct Epoll {
    pub epfd: EpollFd,
    loop_ms: isize,
    handler: Box<Handler<EpollEvent>>,
    buf: Vec<EpollEvent>,
}

impl Epoll {
    pub fn from_fd(epfd: EpollFd, handler: Box<Handler<EpollEvent>>, loop_ms: isize) -> Epoll {
        Epoll {
            epfd: epfd,
            loop_ms: loop_ms,
            handler: handler,
            buf: Vec::with_capacity(*EVENTS_N),
        }
    }

    pub fn new_with<F>(loop_ms: isize, newctl: F) -> Result<Epoll>
        where F: FnOnce(EpollFd) -> Box<Handler<EpollEvent>>
    {

        let fd = try!(epoll_create());

        let epfd = EpollFd { fd: fd };

        let handler = newctl(epfd);

        Ok(Self::from_fd(epfd, handler, loop_ms))
    }

    fn wait(&self, dst: &mut [EpollEvent]) -> Result<usize> {
        trace!("wait()");
        let cnt = try!(epoll_wait(self.epfd.fd, dst, self.loop_ms));
        Ok(cnt)
    }

    #[inline]
    fn run_once(&mut self) -> Result<()> {

        let dst = unsafe { slice::from_raw_parts_mut(self.buf.as_mut_ptr(), self.buf.capacity()) };
        let cnt = try!(self.wait(dst));
        unsafe { self.buf.set_len(cnt) }

        for ev in self.buf.iter() {
            self.handler.ready(&ev);
        }
        Ok(())
    }

    pub fn run(&mut self) -> Result<()> {
        trace!("run()");

        while !self.handler.is_terminated() {
            perror!("loop()", self.run_once());
        }

        Ok(())
    }
}

impl Drop for Epoll {
    fn drop(&mut self) {
        let _ = unistd::close(self.epfd.fd);
    }
}

#[derive(Debug, Copy, Clone)]
pub struct EpollFd {
    pub fd: RawFd,
}

unsafe impl Send for EpollFd {}

impl EpollFd {
    pub fn new(fd: RawFd) -> EpollFd {
        EpollFd { fd: fd }
    }
    fn ctl(&self, op: EpollOp, interest: &EpollEvent, fd: RawFd) -> Result<()> {
        try!(epoll_ctl(self.fd, op, fd, interest));
        Ok(())
    }

    pub fn reregister(&self, fd: RawFd, interest: &EpollEvent) -> Result<()> {
        trace!("reregister()");
        try!(self.ctl(EpollOp::EpollCtlMod, interest, fd));
        Ok(())
    }

    pub fn register(&self, fd: RawFd, interest: &EpollEvent) -> Result<()> {
        trace!("register()");
        try!(self.ctl(EpollOp::EpollCtlAdd, interest, fd));
        Ok(())
    }

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


#[cfg(test)]
mod tests {
    use handler::Handler;
    use error::Result;
    use ::std::sync::mpsc::*;
    use nix::fcntl::{O_NONBLOCK, O_CLOEXEC};
    use nix::unistd;
    use super::*;

    struct ChannelHandler {
        tx: Sender<EpollEvent>,
    }

    impl Handler for ChannelHandler {

        fn ready(&mut self, events: &EpollEvent) -> Result<()> {
            self.tx.send(*events).unwrap();
            Ok(())
        }
    }

    #[test]
    fn notify_handler() {

        let (tx, rx) = channel();

        let loop_ms = 10;

        let mut poll = Epoll::new_with(loop_ms, |_| {
                ChannelHandler {
                    tx: tx,
                }
            })
            .unwrap();

        let (rfd, wfd) = unistd::pipe2(O_NONBLOCK | O_CLOEXEC).unwrap();

        let interest = EpollEvent {
            events: EPOLLONESHOT | EPOLLIN,
            data: rfd as u64,
        };

        unistd::write(wfd, b"hello!").unwrap();

        poll.epfd.register(rfd, &interest).unwrap();

        poll.run_once().unwrap();

        let ev = rx.recv().unwrap();

        assert!(ev.events.contains(EPOLLIN));
        assert!(ev.data == rfd as u64);
    }
}
