use std::net::ToSocketAddrs;
use std::thread;
use std::os::unix::io::RawFd;

use nix::sys::socket::*;
use nix::sys::signalfd::*;
use nix::unistd;

use error::{Error, Result};
use poll::*;
use server::ServerImpl;
use protocol::IOProtocol;

pub struct Mux<P: IOProtocol> {
    srvfd: RawFd,
    cepfd: EpollFd,
    sockaddr: SockAddr,
    max_conn: usize,
    io_threads: usize,
    loop_ms: isize,
    next: usize,
    epfds: Vec<EpollFd>,
    protocol: P,
    terminated: bool,
}

pub struct MuxConfig {
    max_conn: usize,
    io_threads: usize,
    sockaddr: SockAddr,
    loop_ms: isize,
}

impl MuxConfig {
    pub fn new<A: ToSocketAddrs>(addr: A) -> Result<MuxConfig> {
        let inet = try!(addr.to_socket_addrs().unwrap().next().ok_or("could not parse sockaddr"));
        let sockaddr = SockAddr::Inet(InetAddr::from_std(&inet));

        let cpus = ::num_cpus::get();
        let max_conn = 5000 * cpus;
        let io_threads = cpus - 2; //1 for logging/signals + 1 for connection accepting + n

        Ok(MuxConfig {
            sockaddr: sockaddr,
            max_conn: max_conn,
            io_threads: io_threads,
            loop_ms: -1,
        })
    }

    pub fn max_conn(self, max_conn: usize) -> MuxConfig {
        MuxConfig { max_conn: max_conn, ..self }
    }

    pub fn io_threads(self, io_threads: usize) -> MuxConfig {
        MuxConfig { io_threads: io_threads, ..self }
    }

    pub fn loop_ms(self, loop_ms: isize) -> MuxConfig {
        MuxConfig { loop_ms: loop_ms, ..self }
    }
}

impl<P> Mux<P>
    where P: IOProtocol
{
    pub fn new(config: MuxConfig, protocol: P) -> Result<Mux<P>> {

        let MuxConfig { io_threads, max_conn, sockaddr, loop_ms } = config;

        // create connections epoll
        let fd = try!(epoll_create());

        let cepfd = EpollFd { fd: fd };

        // create socket
        let srvfd = try!(socket(AddressFamily::Inet, SockType::Stream, SOCK_NONBLOCK, 0)) as i32;

        setsockopt(srvfd, sockopt::ReuseAddr, &true).unwrap();

        Ok(Mux {
            protocol: protocol,
            sockaddr: sockaddr,
            cepfd: cepfd,
            srvfd: srvfd,
            max_conn: max_conn,
            io_threads: io_threads,
            next: io_threads,
            loop_ms: loop_ms,
            terminated: false,
            epfds: Vec::with_capacity(io_threads),
        })
    }

    fn get_next(&mut self) -> usize {
        if self.next == 0 {
            self.next = self.io_threads;
        }
        self.next -= 1;
        self.next
    }
}

impl<P> ServerImpl for Mux<P>
    where P: IOProtocol + 'static
{
    fn get_loop_ms(&self) -> isize {
        self.loop_ms
    }

    fn stop(&mut self) {
        self.terminated = true;
    }

    fn bind(mut self, mask: SigSet) -> Result<()> {
        trace!("bind()");

        try!(eintr!(bind, "bind", self.srvfd, &self.sockaddr));
        info!("bind: fd {} to {}", self.srvfd, self.sockaddr);

        try!(eintr!(listen, "listen", self.srvfd, self.max_conn));
        info!("listen: fd {} with max connections: {}",
              self.srvfd,
              self.max_conn);

        let ceinfo = EpollEvent {
            events: EPOLLIN | EPOLLOUT | EPOLLERR,
            data: self.srvfd as u64,
        };

        try!(self.cepfd.register(self.srvfd, &ceinfo));

        let io_threads = self.io_threads;
        let loop_ms = self.loop_ms;
        let cepfd = self.cepfd;
        let protocol = self.protocol;

        for i in 0..io_threads {

            let epfd = EpollFd::new(try!(epoll_create()));

            try!(eintr!(bind, "bind", self.srvfd, &self.sockaddr));
            info!("bind: fd {} to {}", self.srvfd, self.sockaddr);

            try!(eintr!(listen, "listen", self.srvfd, self.max_conn));
            info!("listen: fd {} with max connections: {}",
                  self.srvfd,
                  self.max_conn);

            let ceinfo = EpollEvent {
                events: EPOLLIN | EPOLLOUT | EPOLLERR,
                data: self.srvfd as u64,
            };

            try!(epfd.register(self.srvfd, &ceinfo));


            self.epfds.push(epfd);

            thread::spawn(move || {
                // add the set of signals to the signal mask for all threads
                mask.thread_block().unwrap();
                let mut epoll =
                    Epoll::from_fd(epfd, protocol.get_handler(From::from(0_usize), epfd, i), loop_ms);

                perror!("epoll.run()", epoll.run());
            });
        }

        debug!("created {} I/O epoll instances", self.io_threads);


        let mut epoll = Epoll::from_fd(cepfd, protocol.get_handler(From::from(0_usize), cepfd, 0), loop_ms);

        // run accept event loop
        epoll.run()
    }
}


impl<P> Drop for Mux<P>
    where P: IOProtocol
{
    fn drop(&mut self) {
        let _ = unistd::close(self.srvfd).unwrap();
    }
}
