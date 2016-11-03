use std::os::unix::io::{RawFd, AsRawFd};
use std::path::Path;
use std::fs::OpenOptions;
use std::io;
use std::str;
use std::thread;

use rux::handler::Handler;
use rux::buf::ByteBuffer;
use rux::error::Error;
use rux::poll::*;
use rux::fcntl::*;
use rux::stat::*;
use rux::{close, read, write, IOProtocol, Action, Result};

use super::Smeagol;

static OK: &'static [u8] = b"HTTP/1.1 200 OK\r\nAccess-Control-Allow-Headers: origin, content-type, accept\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Max-Age: 1728000\r\nAllow-Control-Allow-Methods: GET,POST,OPTIONS\r\nContent-Type: text/plain\r\nServer: Smeagol/0.1\r\nContent-Length: 0\r\nConnection: keep-alive\r\n\r\n";

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum State {
    // idle or buffering in request
    Ibin,
    // writeable, buffering
    Rin,
    // writeable, parsed
    Rout,
}

pub struct SmeagolHandler {
    state: State,
    currfile: RawFd,
    bufin: ByteBuffer,
    bufout: ByteBuffer,
    elogdirid: usize,
    elogdir: &'static str,
    buffering: usize,
}

// TODO rollback policy + fix alloc
fn next_file(elogdir: &str, elogdirid: usize) -> RawFd {
    open(format!("{}/{}/events.log", elogdir, elogdirid).as_str(),
         O_CREAT | O_WRONLY | O_APPEND | O_CLOEXEC | O_NONBLOCK,
         S_IWUSR)
        .unwrap()
}

impl SmeagolHandler {
    pub fn new(elogdir: &'static str,
               elogdirid: usize,
               buffer_size: usize,
               buffering: usize)
               -> SmeagolHandler {
        trace!("new()");
        SmeagolHandler {
            state: State::Ibin,
            elogdirid: elogdirid,
            elogdir: elogdir,
            currfile: next_file(elogdir, elogdirid),
            bufin: ByteBuffer::with_capacity(buffer_size),
            bufout: ByteBuffer::with_capacity(buffer_size),
            buffering: buffering,
        }
    }

    fn close(&mut self, fd: RawFd) -> Result<()> {
        close(fd);
        if self.bufout.len() > 0 {
            let fd = &self.currfile;
            let cnt = self.on_writable(fd, From::from(&self.bufout));
            trace!("try_log() {} bytes", cnt);
            if cnt > 0 {
                self.bufout.consume(cnt);
            }
        }
        Ok(())
    }

    fn on_error(&mut self, fd: &RawFd) -> Result<()> {
        error!("on_error(): {:?}", fd);
        Ok(())
    }

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

    fn on_writable(&self, fd: &RawFd, bytes: &[u8]) -> usize {
        trace!("on_writable()");

        if let Ok(Some(cnt)) = write(*fd, bytes) {
            trace!("on_writable() bytes {}", cnt);
            return cnt;
        }
        trace!("on_writable(): empty buf");
        0
    }

    fn try_frame(&mut self) -> (bool, usize) {
        trace!("try_frame()");
        let mut headers = [::httparse::EMPTY_HEADER; 16];
        let mut r = ::httparse::Request::new(&mut headers);

        let status = r.parse(From::from(&self.bufin));
        if status.is_err() || !status.unwrap().is_complete() {
            trace!("try_frame(): status {:?}", status);
            return (true, 0);
        }

        let amt = status.unwrap().unwrap();

        let mut length = None;
        let mut connection = None;
        for header in r.headers.into_iter() {
            if header.name == "Content-Length" {
                length = Some(header.value);
            }
            if header.name == "Connection" {
                connection = Some(header.value);
            }
        }

        if length.is_none() {
            trace!("try_frame(): no content length: {:?}", &r.headers);
            // no Content-Length header
            // TODO check rfc
            return (false, amt);
        }

        let len_maybe = str::from_utf8(length.unwrap());
        let conn_maybe = connection.and_then(|s| str::from_utf8(s).ok());

        if len_maybe.is_err() {
            trace!("try_frame(): error decoding length: {:?}", &len_maybe);
            return (false, amt);
        }

        let length_maybe = len_maybe.unwrap().parse();

        if length_maybe.is_err() {
            trace!("try_frame(): error parsing length: {:?}", &length_maybe);
            return (false, amt);
        }

        let length: usize = length_maybe.unwrap();
        let keep_alive = conn_maybe.map(|c| c == "keep-alive").unwrap_or_else(|| false);
        let buflen = self.bufin.len();
        let reqb = length + amt;

        if reqb <= buflen {
            let cnt_maybe = self.bufout.write(&mut self.bufin.slice(amt));

            if cnt_maybe.is_ok() {
                let sum = length + amt;
                trace!("try_frame(): successfully copied payload to bufout: {:?}",
                       &sum);
                return (keep_alive, sum);
            }
            // FIXME bufout might be full
        }
        trace!("try_frame(): could not parse payload: reqb {:?}; buflen {:?}",
               &reqb,
               &buflen);
        (true, 0)
    }

    //  TODO rollback try_log should create new files as it sees fit
    #[inline]
    fn try_log(&mut self) -> usize {
        let buflen = self.bufout.len();
        if buflen > self.buffering {
            let fd = &self.currfile;
            let cnt = self.on_writable(fd, From::from(&self.bufout));
            trace!("try_log() {} bytes", cnt);
            if cnt > 0 {
                self.bufout.consume(cnt);
                return cnt;
            }
        }
        trace!("try_log() 0 bytes written: buffer len: {:?}", buflen);
        0
    }

    #[inline]
    fn ok(&mut self, fd: &RawFd) -> usize {
        trace!("ok()");
        self.on_writable(fd, OK)
    }
}

impl Handler<EpollEvent> for SmeagolHandler {
    fn is_terminated(&self) -> bool {
        false
    }

    fn ready(&mut self, event: &EpollEvent) {

        let kind = event.events;
        let keep = true;

        let fd = match Smeagol.decode(event.data) {
            Action::New(_, fd) => fd,
            Action::Notify(_, fd) => fd,
        };

        let mut next: State = self.state;

        if kind.contains(EPOLLRDHUP) || kind.contains(EPOLLHUP) {
            trace!("socket fd {}: EPOLLHUP", &fd);
            perror!("close()", self.close(fd));
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
            let (keep, cnt) = self.try_frame();
            if cnt > 0 {
                self.bufin.consume(cnt);
                self.try_log();
                trace!("{}: state is ROUT", fd);
                next = State::Rout;
            }
        }

        if kind.contains(EPOLLOUT) {
            trace!("socket fd {}: EPOLLOUT", &fd);
            if (self.state == State::Rout || next == State::Rout) && self.ok(&fd) > 0 {
                trace!("{}: state is IBIN", fd);
                next = State::Ibin;
                // FIXME
                // perror!("close()", self.close(fd));
            } else {
                trace!("{}: state is RIN", fd);
                next = State::Rin;
            }
        }


        if self.state == State::Ibin {
            trace!("{}: changing state from {:?} to {:?}",
                   fd,
                   &self.state,
                   next);
            // TODO reregister let action = Action::Notify(next, fd);

            // let interest = EpollEvent {
            //     events: EPOLLIN | EPOLLOUT | EPOLLHUP | EPOLLRDHUP | EPOLLET,
            //     data: self.eproto.encode(action),
            // };

            // match self.epfd.reregister(fd, &interest) {
            //     Ok(_) => {}
            //     Err(e) => report_err!("reregister()", e),
            // }
        }

        self.state = next;
    }
}
