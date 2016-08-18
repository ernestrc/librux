use std::net::ToSocketAddrs;
use std::thread;
use std::os::unix::io::RawFd;

use nix::sys::socket::*;
use nix::sys::signalfd::*;
use nix::unistd;

use error::{Error, Result};
use controller::*;
use controller::sync::*;
use poll::*;
use server::ServerImpl;

/// Simple server implementation that creates one AF_INET/SOCK_STREAM socket and uses it to bind/listen
/// at the specified address. It instantiates one epoll to accept new connections
/// and one instance per cpu left to perform I/O.
///
/// Events are handled synchronously TODO explain
///
/// New connections are load balanced from the connections epoll to the rest in a round-robin fashion.
pub struct SimpleMux<P: EpollProtocol> {
    srvfd: RawFd,
    cepfd: EpollFd,
    sockaddr: SockAddr,
    eproto: P,
    max_conn: usize,
    io_threads: usize,
    loop_ms: isize,
    accepted: u64,
    terminated: bool,
    epfds: Vec<EpollFd>,
}

pub struct SimpleMuxConfig {
    max_conn: usize,
    io_threads: usize,
    sockaddr: SockAddr,
    loop_ms: isize,
}

impl SimpleMuxConfig {

    pub fn new<A: ToSocketAddrs>(addr: A) -> Result<SimpleMuxConfig> {
        let inet = try!(addr.to_socket_addrs().unwrap().next().ok_or("could not parse sockaddr"));
        let sockaddr = SockAddr::Inet(InetAddr::from_std(&inet));

        // TODO provide good default depending on the number of I/O threads
        let max_conn = 50_000;
        let cpus = ::num_cpus::get();
        let io_threads = cpus - 2; //1 for logging/signals + 1 for connection accepting + n

        Ok(SimpleMuxConfig {
            sockaddr: sockaddr,
            max_conn: max_conn,
            io_threads: io_threads,
            loop_ms: -1,
        })
    }

    pub fn max_conn(self, max_conn: usize) -> SimpleMuxConfig {
        SimpleMuxConfig { max_conn: max_conn, ..self }
    }

    pub fn io_threads(self, io_threads: usize) -> SimpleMuxConfig {
        SimpleMuxConfig { io_threads: io_threads, ..self }
    }

    pub fn loop_ms(self, loop_ms: isize) -> SimpleMuxConfig {
        SimpleMuxConfig { loop_ms: loop_ms, ..self }
    }
}

impl<P: EpollProtocol> SimpleMux<P> {
    pub fn new(eproto: P, config: SimpleMuxConfig) -> Result<SimpleMux<P>> {

        let SimpleMuxConfig { io_threads, max_conn, sockaddr, loop_ms } = config;

        // create connections epoll
        let fd = try!(epoll_create());

        let cepfd = EpollFd { fd: fd };

        // create socket
        let srvfd = try!(socket(AddressFamily::Inet, SockType::Stream, SOCK_NONBLOCK | SOCK_CLOEXEC, 0)) as i32;

        Ok(SimpleMux {
            eproto: eproto,
            sockaddr: sockaddr,
            cepfd: cepfd,
            srvfd: srvfd,
            max_conn: max_conn,
            io_threads: io_threads,
            loop_ms: loop_ms,
            terminated: false,
            accepted: 0,
            epfds: Vec::with_capacity(io_threads),
        })
    }
}

impl <P: EpollProtocol> Controller for SimpleMux<P> {

    fn is_terminated(&self) -> bool {
        self.terminated
    }

    fn ready(&mut self, _: &EpollEvent) -> Result<()> {
        trace!("new()");

        //only monitoring events from srvfd
        match eintr!(accept4, "accept4", self.srvfd, SOCK_NONBLOCK | SOCK_CLOEXEC) {
            Ok(Some(clifd)) => {

                trace!("accept4: acceted new tcp client {}", &clifd);

                // round robin: TODO use iterator
                let next = (self.accepted % self.io_threads as u64) as usize;

                let epfd: EpollFd = *self.epfds.get(next).unwrap();

                let info = EpollEvent {
                    events: EPOLLONESHOT | EPOLLIN | EPOLLOUT | EPOLLHUP | EPOLLRDHUP,
                    // start with EpollProtocol handler 0
                    data: self.eproto.encode(Action::New(0.into(), clifd)),
                };

                debug!("assigned accepted client {} to epoll instance {}", &clifd, &epfd);

                perror!("epoll_ctl", epfd.register(clifd, &info));

                trace!("epoll_ctl: registered interests for {}", clifd);

                self.accepted += 1;
            }
            Ok(None) => debug!("accept4: socket not ready"),
            Err(e) => error!("accept4: {}", e),
        }
        Ok(())
    }
}

impl<P> ServerImpl for SimpleMux<P>
where P: EpollProtocol + Sync + 'static
{
    fn get_loop_ms(&self) -> isize {
        self.loop_ms
    }

    fn bind(mut self, mask: SigSet) -> Result<()> {
        trace!("bind()");

        try!(eagain!(bind, "bind", self.srvfd, &self.sockaddr));
        info!("bind: fd {} to {}", self.srvfd, self.sockaddr);

        try!(eagain!(listen, "listen", self.srvfd, self.max_conn));
        info!("listen: fd {} with max connections: {}", self.srvfd, self.max_conn);

        let ceinfo = EpollEvent {
            events: EPOLLIN | EPOLLOUT | EPOLLERR,
            data: self.srvfd as u64,
        };

        try!(self.cepfd.register(self.srvfd, &ceinfo));

        let max_conn = self.max_conn;
        let io_threads = self.io_threads;
        let eproto = self.eproto;
        let loop_ms = self.loop_ms;
        let cepfd = self.cepfd;

        for _ in 0..io_threads {

            let epfd = EpollFd::new(try!(epoll_create()));

            self.epfds.push(epfd);

            thread::spawn(move || {
                // add the set of signals to the signal mask for all threads
                // otherwise signalfd will not work properlys
                mask.thread_block().unwrap();
                // max number of handlers (connections)
                // per controller
                let maxh = max_conn / io_threads;
                let controller = SyncController::new(epfd, eproto, maxh);

                let mut epoll = Epoll::from_fd(epfd, controller, -1);

                perror!("epoll.run()", epoll.run());
            });
        }

        debug!("created {} I/O epoll instances", self.io_threads);


        //consume itself
        let mut epoll = Epoll::from_fd(cepfd, self, loop_ms);

        //run accept event loop
        epoll.run()
    }

}


impl<P> Drop for SimpleMux<P>
where P: EpollProtocol
{
    fn drop(&mut self) {
        let _ = unistd::close(self.srvfd).unwrap();
    }
}
