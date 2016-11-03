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
use rux::{close as rclose, read, Shutdown, shutdown as rshutdown, write, IOProtocol,
          Action, Result, Slab, Entry};

use super::Smeagol;

static OK: &'static [u8] = b"HTTP/1.1 200 OK\r\nAccess-Control-Allow-Headers: origin, content-type, accept\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Max-Age: 1728000\r\nAllow-Control-Allow-Methods: GET,POST,OPTIONS\r\nContent-Type: text/plain\r\nServer: Smeagol/0.1\r\nContent-Length: 0\r\nConnection: keep-alive\r\n\r\n";
static ERR: &'static [u8] = b"HTTP/1.1 500 Internal Server Error\r\nAccess-Control-Allow-Headers: origin, content-type, accept\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Max-Age: 1728000\r\nAllow-Control-Allow-Methods: GET,POST,OPTIONS\r\nContent-Type: text/plain\r\nServer: Smeagol/0.1\r\nContent-Length: 0\r\nConnection: keep-alive\r\n\r\n";

#[derive(Clone, Copy, PartialEq, Debug)]
enum State {
    Idle,
    ReadyOut,
    Parsed(bool),
    Shutdown,
    Closed,
    Close,
    Error(bool),
    Reset,
}

#[derive(Debug)]
struct Connection {
    pub state: State,
    fd: RawFd,
}


impl Connection {
    fn new(fd: RawFd) -> Connection {
        Connection {
            state: State::Idle,
            fd: fd,
        }
    }

    fn state(&mut self, state: State) -> State {
        self.state = state;
        self.state
    }

    fn ready(&mut self,
             event: &EpollEvent,
             bufin: &mut ByteBuffer,
             bufout: &mut ByteBuffer)
             -> State {
        let kind = event.events;

        if kind.contains(EPOLLRDHUP) || kind.contains(EPOLLHUP) {
            trace!("socket fd {}: EPOLLHUP", &self.fd);
            return self.state(State::Close);
        }

        if kind.contains(EPOLLIN) {
            trace!("socket fd {}: EPOLLIN", &self.fd);
            match read(self.fd, From::from(&mut *bufin)) {
                Ok(Some(cnt)) => {
                    if cnt > 0 {
                        bufin.extend(cnt);
                    } else {
                        self.state(State::Closed);
                    }
                }
                Ok(None) => {},
                Err(e) => {
                    error!("read fd {}: {:?}", self.fd, e);
                    self.state(State::Error(false));
                }
            }
            let cnt = self.try_frame(bufin, bufout);
            if cnt > 0 {
                bufin.consume(cnt);
            }
        }

        if kind.contains(EPOLLERR) {
            error!("socket fd {}: EPOLLERR", &self.fd);
            self.state(State::Error(false));
        }

        if kind.contains(EPOLLOUT) {
            trace!("socket fd {}: EPOLLOUT: {:?}", &self.fd, &self.state);
            match self.state {
                State::Parsed(keep_alive) => self.ok(keep_alive),
                State::Error(keep_alive) => self.error(keep_alive),
                State::Idle => self.state(State::ReadyOut),
                e => e,
            };
        }

        self.state
    }

    fn try_frame(&mut self, bufin: &mut ByteBuffer, bufout: &mut ByteBuffer) -> usize {
        trace!("try_frame()");
        let mut headers = [::httparse::EMPTY_HEADER; 16];
        let mut r = ::httparse::Request::new(&mut headers);

        let status = match r.parse(From::from(&*bufin)) {
            Err(e) => {
                error!("parse http request error: {:?}", e);
                self.state(State::Error(false));
                return 0;
            }
            Ok(s) => s,
        };

        if !status.is_complete() {
            return 0;
        }

        let amt = status.unwrap();

        let mut length = None;
        let mut connection = None;
        for header in r.headers.into_iter() {
            if header.name == "Content-Length" || header.name == "Content-length" {
                length = Some(header.value);
            }
            if header.name == "Connection" {
                connection = Some(header.value);
            }
        }

        if length.is_none() {
            trace!("try_frame(): no content length: {:?}", &r.headers);
            // no Content-Length header
            self.state(State::Reset);
            return 0;
        }

        let len_maybe = str::from_utf8(length.unwrap());
        let conn_maybe = connection.and_then(|s| str::from_utf8(s).ok());

        if len_maybe.is_err() {
            error!("error decoding content length header value: {:?}", &len_maybe);
            self.state(State::Error(false));
            return amt;
        }

        let length_maybe = len_maybe.unwrap().parse();

        if length_maybe.is_err() {
            error!("error parsing length: {:?}", &length_maybe);
            self.state(State::Error(false));
            return amt;
        }

        let length: usize = length_maybe.unwrap();
        let keep_alive = conn_maybe.map(|c| c == "keep-alive").unwrap_or_else(|| false);
        let buflen = bufin.len();
        let reqb = length + amt;

        if reqb <= buflen {
            match bufout.write(&mut bufin.slice(amt)) {
                Ok(_) => {
                    let sum = length + amt;
                    trace!("try_frame(): successfully copied payload to bufout: {:?}",
                           &sum);
                    self.state(State::Parsed(keep_alive));
                    return sum;
                }
                // TODO
                Err(e) => {
                    error!("failed to copy data to bufout {:?}", e);
                    self.state(State::Error(keep_alive));
                }
            }
        }
        trace!("try_frame(): could not parse payload: reqb {:?}; buflen {:?}",
               &reqb,
               &buflen);
        0
    }

    fn respond(&mut self, keep_alive: bool, msg: &'static [u8]) -> State {
        trace!("respond(): keep-alive: {:?}", &keep_alive);
        let result = write(self.fd, msg);

        if result.is_err() {
            error!("error writing http response {:?}", result);
            return self.state(State::Error(keep_alive));
        }

        let w = result.unwrap();

        if w.is_none() {
            return self.state(State::Parsed(keep_alive));
        }

        if keep_alive {
            return self.state(State::Reset);
        }

        self.state(State::Shutdown)
    }

    fn ok(&mut self, keep_alive: bool) -> State {
        self.respond(keep_alive, OK)
    }

    fn error(&mut self, keep_alive: bool) -> State {
        error!("500 INTERNAL SERVER ERROR: {:?}", self.fd);
        self.respond(keep_alive, ERR)
    }
}

pub struct SmeagolHandler {
    epfd: EpollFd,
    buffers: Vec<ByteBuffer>,
    connections: Slab<Connection, usize>,
    currfile: RawFd,
    bufout: ByteBuffer,
    elogdirid: usize,
    elogdir: &'static str,
    buffering: usize,
}

// TODO rollback policy + fix alloc
fn next_file(elogdir: &str, elogdirid: usize) -> RawFd {
    open(format!("{}/{}/events.log", elogdir, elogdirid).as_str(),
         O_CREAT | O_WRONLY | O_APPEND,
         S_IWUSR)
        .unwrap()
}

impl SmeagolHandler {
    pub fn new(elogdir: &'static str,
               elogdirid: usize,
               ibuffersize: usize,
               obuffersize: usize,
               buffering: usize,
               max_conn: usize,
               epfd: EpollFd)
               -> SmeagolHandler {
        trace!("new()");
        SmeagolHandler {
            elogdirid: elogdirid,
            elogdir: elogdir,
            currfile: next_file(elogdir, elogdirid),
            bufout: ByteBuffer::with_capacity(obuffersize),
            buffering: buffering,
            connections: Slab::with_capacity(max_conn),
            buffers: vec!(ByteBuffer::with_capacity(ibuffersize); max_conn),
            epfd: epfd,
        }
    }

    fn shutdown(&mut self, fd: RawFd, idx: usize) {
        trace!("shutting down {} on {}", &fd, &idx);
        // FIXME, for now ignore ENOTCONN
        rshutdown(fd, Shutdown::Write);
    }

    fn close(&mut self, fd: RawFd, idx: usize) {
        trace!("closing {} on {}", &fd, &idx);
        perror!("{}", rclose(fd));
        self.connections.remove(idx);
        self.buffers[idx].clear();
    }

    //  TODO rollback try_log should create new files as it sees fit
    //  TODO timerfd to flush periodically
    #[inline]
    fn log(&mut self) -> usize {
        let buflen = self.bufout.len();

        if buflen < self.buffering {
            trace!("log() 0 bytes written: buffer len: {:?}", buflen);
            return 0;
        }

        let fd = self.currfile;
        // TODO alignment!
        let res = write(fd, From::from(&self.bufout));

        if res.is_err() {
            return 0;
        }

        let w = res.unwrap();

        if w.is_none() {
            return 0;
        }

        let cnt: usize = w.unwrap();
        trace!("log() {} bytes", cnt);

        if cnt > 0 {
            self.bufout.consume(cnt);
        }
        return cnt;
    }
}

fn decode(epfd: &EpollFd,
          event: &EpollEvent,
          connections: &mut Slab<Connection, usize>)
          -> ::std::result::Result<(usize, RawFd), (Error, RawFd)> {
    match Smeagol.decode(event.data) {
        Action::New(_, fd) => {
            match connections.vacant_entry() {
                Some(entry) => {

                    let i = entry.index();
                    let conn = Connection::new(fd);
                    entry.insert(conn);

                    let action = Action::Notify(i, fd);
                    let interest = EpollEvent {
                        events: EPOLLIN | EPOLLOUT | EPOLLHUP | EPOLLRDHUP | EPOLLET,
                        data: Smeagol.encode(action),
                    };

                    match epfd.reregister(fd, &interest) {
                        Ok(_) => {}
                        Err(e) => return Err((e, fd)),
                    };

                    Ok((i, fd))
                }
                None => Err(("connection slab full".into(), fd)),
            }
        }
        Action::Notify(i, fd) => Ok((i, fd)),
    }
}

impl Handler<EpollEvent> for SmeagolHandler {
    fn is_terminated(&self) -> bool {
        false
    }

    fn ready(&mut self, event: &EpollEvent) {

        let (idx, fd) = match decode(&self.epfd, event, &mut self.connections) {
            Ok((idx, fd)) => (idx, fd),
            Err((e, fd)) => {
                perror!("{}", self.epfd.unregister(fd));
                error!("closing: {:?}", e);
                perror!("{}", rclose(fd));
                return;
            }
        };

        match self.connections[idx].ready(event, &mut self.buffers[idx], &mut self.bufout) {
            State::Idle => {}
            State::ReadyOut => {}
            State::Parsed(_) => {}
            State::Error(_) => {}
            State::Close => self.close(fd, idx),
            State::Closed => self.close(fd, idx),
            State::Shutdown => self.shutdown(fd, idx),
            State::Reset => {
                self.buffers[idx].clear();
                self.connections[idx].state(State::Idle);
            }
        };

        self.log();
    }
}
