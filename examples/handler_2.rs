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
use rux::{close as rclose, read as rread, write as rwrite, IOProtocol, Action, Result, Slab, Entry};

use super::Smeagol;

static OK: &'static [u8] = b"HTTP/1.1 200 OK\r\nAccess-Control-Allow-Headers: origin, content-type, accept\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Max-Age: 1728000\r\nAllow-Control-Allow-Methods: GET,POST,OPTIONS\r\nContent-Type: text/plain\r\nServer: Smeagol/0.1\r\nContent-Length: 0\r\nConnection: keep-alive\r\n\r\n";
static ERR: &'static [u8] = b"HTTP/1.1 500 Internal Server Error\r\nAccess-Control-Allow-Headers: origin, content-type, accept\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Max-Age: 1728000\r\nAllow-Control-Allow-Methods: GET,POST,OPTIONS\r\nContent-Type: text/plain\r\nServer: Smeagol/0.1\r\nContent-Length: 0\r\nConnection: keep-alive\r\n\r\n";

#[derive(Clone, Copy, PartialEq, Debug)]
enum State {
    Idle,
    ReadyOut,
    Parsed(bool),
    Close,
    Error(bool),
    Reset,
}

#[derive(Debug)]
struct Connection {
    pub state: State,
    fd: RawFd,
}

fn write(fd: &RawFd, bytes: &[u8]) -> usize {
    trace!("write()");

    if let Ok(Some(cnt)) = rwrite(*fd, bytes) {
        trace!("write() bytes {}", cnt);
        return cnt;
    }
    trace!("write(): empty buf");
    0
}

fn read(fd: &RawFd, buffer: &mut ByteBuffer) -> usize {
    trace!("read()");

    if let Ok(Some(n)) = rread(*fd, From::from(&mut *buffer)) {
        trace!("read(): {:?} bytes", n);
        buffer.extend(n);
        return n;
    }

    trace!("read(): socket not ready");
    0
}


impl Connection {
    fn new(fd: RawFd) -> Connection {
        Connection {
            state: State::Idle,
            fd: fd,
        }
    }

    fn state(&mut self, state: State) -> State {
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

        if kind.contains(EPOLLERR) {
            trace!("socket fd {}: EPOLLERR", &self.fd);
            return self.state(State::Close);
        }

        if kind.contains(EPOLLIN) {
            trace!("socket fd {}: EPOLLIN", &self.fd);
            read(&self.fd, bufin);
            let cnt = self.try_frame(bufin, bufout);
            if cnt > 0 {
                bufin.consume(cnt);
            }
        }

        if kind.contains(EPOLLOUT) {
            trace!("socket fd {}: EPOLLOUT", &self.fd);
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
            self.state(State::Reset);
            return 0;
        }

        let len_maybe = str::from_utf8(length.unwrap());
        let conn_maybe = connection.and_then(|s| str::from_utf8(s).ok());

        if len_maybe.is_err() {
            trace!("try_frame(): error decoding length: {:?}", &len_maybe);
            self.state = State::Error(false);
            return amt;
        }

        let length_maybe = len_maybe.unwrap().parse();

        if length_maybe.is_err() {
            trace!("try_frame(): error parsing length: {:?}", &length_maybe);
            self.state = State::Error(false);
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
                    self.state(State::Error(keep_alive));
                    error!("failed to copy data to bufout {:?}", e)
                }
            }
        }
        trace!("try_frame(): could not parse payload: reqb {:?}; buflen {:?}",
               &reqb,
               &buflen);
        0
    }

    fn respond(&mut self, keep_alive: bool, msg: &'static [u8]) -> State {
        write(&self.fd, OK);
        if keep_alive {
            return self.state(State::Reset);
        }

        self.state(State::Close)
    }

    fn ok(&mut self, keep_alive: bool) -> State {
        self.respond(keep_alive, OK)
    }

    fn error(&mut self, keep_alive: bool) -> State {
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
         O_CREAT | O_WRONLY | O_APPEND | O_CLOEXEC | O_NONBLOCK,
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

    fn close(&mut self, fd: RawFd, idx: usize) {
        rclose(fd);
        self.epfd.unregister(fd);
        self.connections.remove(idx);
        self.buffers[idx].clear();
    }

    //  TODO rollback try_log should create new files as it sees fit
    //  TODO timerfd
    #[inline]
    fn log(&mut self) -> usize {
        let buflen = self.bufout.len();
        if buflen > self.buffering {
            let fd = &self.currfile;
            // TODO alignment!
            let cnt = write(fd, From::from(&self.bufout));
            trace!("log() {} bytes", cnt);
            if cnt > 0 {
                self.bufout.consume(cnt);
                return cnt;
            }
        }
        trace!("log() 0 bytes written: buffer len: {:?}", buflen);
        0
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
                error!("{:?}", e);
                perror!("{}", rclose(fd));
                perror!("{}", self.epfd.unregister(fd));
                return;
            }
        };

        match self.connections[idx].ready(event, &mut self.buffers[idx], &mut self.bufout) {
            State::Idle => {}
            State::ReadyOut => {}
            State::Parsed(_) => {}
            State::Error(_) => {}
            State::Close => self.close(fd, idx),
            State::Reset => {
                self.buffers[idx].clear();
                self.connections[idx].state(State::Idle);
            }
        };

        self.log();
    }
}
