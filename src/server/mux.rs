use std::net::ToSocketAddrs;
use std::thread;
use std::os::unix::io::RawFd;
use std::marker::PhantomData;

use nix::sys::socket::*;
use nix::sys::signalfd::*;
use nix::unistd;

use error::{Error, Result};
use poll::*;
use protocol::StaticProtocol;
use server::Bind;
use handler::Handler;

/// Server implementation that creates one AF_INET/SOCK_STREAM socket and uses it to bind/listen
/// at the specified address. It will create one handler/epoll instance per `io_thread` plus one for the main thread.
pub struct Mux<H, P>
    where H: Handler<EpollEvent>,
          P: StaticProtocol<H> + 'static
{
    srvfd: RawFd,
    cepfd: EpollFd,
    sockaddr: SockAddr,
    max_conn: usize,
    io_threads: usize,
    epoll_config: EpollConfig,
    epfds: Vec<EpollFd>,
    terminated: bool,
    protocol: P,
    d: PhantomData<H>,
}

pub struct MuxConfig {
    max_conn: usize,
    io_threads: usize,
    sockaddr: SockAddr,
    epoll_config: EpollConfig,
}

impl MuxConfig {
    pub fn new<A: ToSocketAddrs>(addr: A) -> Result<MuxConfig> {
        let inet = try!(addr.to_socket_addrs().unwrap().next().ok_or("could not parse sockaddr"));
        let sockaddr = SockAddr::Inet(InetAddr::from_std(&inet));

        let cpus = ::num_cpus::get();
        let max_conn = 5000 * cpus;
        let io_threads = if cpus > 3 {
            cpus - 2 //1 for logging/signals + 1 main thread
        } else {
            1
        };

        Ok(MuxConfig {
            sockaddr: sockaddr,
            max_conn: max_conn,
            io_threads: io_threads,
            epoll_config: Default::default()
        })
    }

    pub fn max_conn(self, max_conn: usize) -> MuxConfig {
        MuxConfig { max_conn: max_conn, ..self }
    }

    pub fn io_threads(self, io_threads: usize) -> MuxConfig {
        MuxConfig { io_threads: io_threads, ..self }
    }

    pub fn epoll_config(self, epoll_config: EpollConfig) -> MuxConfig {
        MuxConfig { epoll_config: epoll_config, ..self }
    }
}

impl<H, P> Mux<H, P>
    where H: Handler<EpollEvent>,
          P: StaticProtocol<H>
{
    pub fn new(config: MuxConfig, protocol: P) -> Result<Mux<H, P>> {

        let MuxConfig { io_threads, max_conn, sockaddr, epoll_config } = config;

        // create connections epoll
        let fd = try!(epoll_create());

        let cepfd = EpollFd { fd: fd };

        // create socket
        let srvfd = try!(socket(AddressFamily::Inet, SockType::Stream, SOCK_NONBLOCK, 0)) as i32;

        setsockopt(srvfd, sockopt::ReuseAddr, &true).unwrap();

        Ok(Mux {
            sockaddr: sockaddr,
            cepfd: cepfd,
            protocol: protocol,
            srvfd: srvfd,
            max_conn: max_conn,
            io_threads: io_threads,
            epoll_config: epoll_config,
            terminated: false,
            epfds: Vec::with_capacity(io_threads),
            d: PhantomData {},
        })
    }
}

impl<H, P> Bind for Mux<H, P>
    where H: Handler<EpollEvent>,
          P: StaticProtocol<H>
{
    fn get_epoll_config(&self) -> EpollConfig {
        self.epoll_config
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
        trace!("registered main thread's interest on {}", self.srvfd);

        let io_threads = self.io_threads;
        let epoll_config = self.epoll_config;
        let cepfd = self.cepfd;
        let srvfd = self.srvfd;
        let protocol = self.protocol;

        // id 0 is for main thread's handler
        for i in 1..io_threads + 1 {

            let epfd = EpollFd::new(try!(epoll_create()));

            let ceinfo = EpollEvent {
                events: EPOLLIN | EPOLLOUT | EPOLLERR | EPOLLEXCLUSIVE | EPOLLWAKEUP,
                data: srvfd as u64,
            };

            try!(epfd.register(srvfd, &ceinfo));
            trace!("registered thread's {} interest on {}", i, srvfd);


            self.epfds.push(epfd);

            thread::spawn(move || {
                trace!("spawned new thread {}", i);
                // add the set of signals to the signal mask for all threads
                mask.thread_block().unwrap();
                let mut epoll = Epoll::from_fd(epfd,
                                               protocol.get_handler(From::from(0_usize), epfd, i),
                                               epoll_config);

                info!("starting thread's {} event loop", i);
                perror!("epoll.run()", epoll.run());
            });
        }

        debug!("created {} I/O epoll instances", self.io_threads);

        let mut epoll = Epoll::from_fd(cepfd,
                                       protocol.get_handler(From::from(0_usize), cepfd, 0),
                                       epoll_config);

        info!("starting main event loop");
        epoll.run()
    }
}


impl<H, P> Drop for Mux<H, P>
    where H: Handler<EpollEvent>,
          P: StaticProtocol<H>
{
    fn drop(&mut self) {
        let _ = unistd::close(self.srvfd).unwrap();
    }
}
