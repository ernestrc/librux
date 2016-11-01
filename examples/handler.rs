use std::os::unix::io::RawFd;

use rux::handler::Handler;
use rux::buf::ByteBuffer;
use rux::error::Error;
use rux::poll::*;
use rux::fcntl::*;
use rux::stat::*;
use rux::{close, read, write, IOProtocol, Action, Result};

const BUF_CAP: usize = 1024 * 1024;
const OK: &'static [u8] = b"HTTP/1.1 200 OK\r\nAccess-Control-Allow-Headers: origin, content-type, accept\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Max-Age: 1728000\r\nAllow-Control-Allow-Methods: POST\r\nContent-Type: text/plain\r\nServer: Smeagol/0.1\r\nContent-Length: 0\r\nConnection: keep-alive\r\n\r\n";

#[derive(Clone, Copy)]
pub struct SmeagolProto;

impl IOProtocol for SmeagolProto {
    type Protocol = usize;

    fn get_handler(&self, _: usize, epfd: EpollFd) -> Box<Handler<EpollEvent>> {
        Box::new(SmeagolHandler::new(epfd))
    }
}


pub struct SmeagolHandler {
    logs: RawFd,
    epfd: EpollFd,
    eproto: SmeagolProto,
    bufin: ByteBuffer,
    bufout: ByteBuffer,
}

impl SmeagolHandler {
    pub fn new(epfd: EpollFd) -> SmeagolHandler {
        trace!("new()");
        let eproto = SmeagolProto;
        let logsfd = open("/tmp/smeagol/events.log",
                          O_CREAT | O_WRONLY | O_APPEND | O_CLOEXEC,
                          S_IWUSR)
            .unwrap();
        SmeagolHandler {
            epfd: epfd,
            eproto: eproto,
            logs: logsfd,
            bufin: ByteBuffer::with_capacity(BUF_CAP),
            bufout: ByteBuffer::with_capacity(BUF_CAP),
        }
    }

    #[inline]
    fn on_error(&mut self, fd: &RawFd) -> Result<()> {
        error!("on_error(): {:?}", fd);
        Ok(())
    }

    #[inline]
    fn on_readable(&mut self, fd: &RawFd) -> usize {
        trace!("on_readable()");

        if let Ok(Some(n)) = read(*fd, From::from(&mut self.bufin)) {
            trace!("on_readable(): {:?} bytes", n);
            self.bufin.extend(n);
            return n;
        }

        trace!("on_readable(): socket not ready");
        0
    }

    #[inline]
    fn on_writable(&self, fd: &RawFd, bytes: &[u8]) -> usize {
        trace!("on_writable()");

        if let Ok(Some(cnt)) = write(*fd, bytes) {
            trace!("on_writable() bytes {}", cnt);
            return cnt;
        }
        trace!("on_writable(): empty buf");
        0
    }

    #[inline]
    fn try_frame(&mut self) -> usize {
        // TODO get data
        if !self.bufin.is_empty() {
            let mut r = ::httparse::Request::new(&mut []);
            let status = r.parse(From::from(&self.bufin));
            let amt = match status {
                Ok(::httparse::Status::Complete(amt)) => amt,
                Ok(::httparse::Status::Partial) => return 0,
                Err(_) => return 0,// FIXME
            };
        }
        0
    }

    #[inline]
    fn try_log(&mut self) -> usize {
        let non_empty = !self.bufout.is_empty();
        trace!("try_log() non_empty: {}", non_empty);
        if non_empty {
            let fd = &self.logs;
            let cnt = self.on_writable(fd, From::from(&self.bufout));
            trace!("try_log() {} bytes", cnt);
            if cnt > 0 {
                self.bufout.consume(cnt);
                return cnt;
            }
        }
        trace!("try_log() 0 bytes");
        0
    }

    #[inline]
    fn ok(&mut self, fd: &RawFd) -> usize {
        trace!("ok()");
        self.on_writable(fd, OK)
    }
}

// idle, buffering in request
const IBIN: &'static usize = &1;
// ready to respond but buffering
const RIN: &'static usize = &2;
// ready to respond
const ROUT: &'static usize = &3;

impl Handler<EpollEvent> for SmeagolHandler {
    fn is_terminated(&self) -> bool {
        false
    }

    fn ready(&mut self, event: &EpollEvent) {

        let kind = event.events;
        let mut next: usize = 0;

        let (state, fd) = match self.eproto.decode(event.data) {
            Action::New(_, fd) => (*IBIN, fd),
            Action::Notify(s, fd) => (s, fd),
        };

        if kind.contains(EPOLLRDHUP) || kind.contains(EPOLLHUP) {
            trace!("socket fd {}: EPOLLHUP", &fd);
            close(fd);
            return;
        }

        if kind.contains(EPOLLERR) {
            trace!("socket fd {}: EPOLLERR", &fd);
            perror!("on_error()", self.on_error(&fd));
            return;
        }

        if kind.contains(EPOLLIN) {
            trace!("socket fd {}: EPOLLIN", fd);
            self.on_readable(&fd);
            let cnt = self.try_frame();
            if cnt > 0 {
                self.bufin.consume(cnt);
                self.try_log();
                trace!("{}: state is ROUT", fd);
                next = *ROUT;
            }
        }

        if kind.contains(EPOLLOUT) {
            trace!("socket fd {}: EPOLLOUT", &fd);
            if state == *ROUT || next == *ROUT {
                if self.ok(&fd) > 0 {
                    trace!("{}: state is IBIN", fd);
                    next = *IBIN;
                }
            } else {
                trace!("{}: state is RIN", fd);
                next = *RIN;
            }
        }

        if state != next {
            trace!("{}: changing state from {} to {}", fd, state, next);
            let action = Action::Notify(next, fd);

            let interest = EpollEvent {
                events: EPOLLIN | EPOLLOUT | EPOLLHUP | EPOLLRDHUP | EPOLLET,
                data: self.eproto.encode(action),
            };

            match self.epfd.reregister(fd, &interest) {
                Ok(_) => {}
                Err(e) => report_err!("reregister()", e),
            }
        }
    }
}
