use std::net::ToSocketAddrs;
use std::thread;
use std::os::unix::io::RawFd;
use std::marker::PhantomData;

use nix::sys::socket::*;
use nix::sys::signalfd::*;
use nix::unistd;
use nix::sched;

use error::*;
use poll::*;
use protocol::StaticProtocol;
use prop::Server;
use handler::Handler;

/// Server implementation that creates one AF_INET/SOCK_STREAM socket and uses it to bind/listen
/// at the specified address. It will create one handler/epoll instance per `io_thread` plus one for the main thread.
pub struct Mux<H, P>
    where H: Handler<EpollEvent>,
          P: StaticProtocol<EpollEvent, H> + 'static
{
    srvfd: RawFd,
    cepfd: EpollFd,
    sockaddr: SockAddr,
    max_conn: usize,
    io_threads: usize,
    epoll_config: EpollConfig,
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
        let io_threads = if cpus > 2 {
            cpus - 1 //1 for logging/signals
        } else {
            1
        };

        Ok(MuxConfig {
            sockaddr: sockaddr,
            max_conn: max_conn,
            io_threads: io_threads,
            epoll_config: Default::default(),
        })
    }

    pub fn max_conn(self, max_conn: usize) -> MuxConfig {
        MuxConfig { max_conn: max_conn, ..self }
    }

    pub fn io_threads(self, io_threads: usize) -> MuxConfig {
        assert!(io_threads > 0, "I/O threads must be greater than 0");
        MuxConfig { io_threads: io_threads, ..self }
    }

    pub fn epoll_config(self, epoll_config: EpollConfig) -> MuxConfig {
        MuxConfig { epoll_config: epoll_config, ..self }
    }
}

impl<H, P> Mux<H, P>
    where H: Handler<EpollEvent>,
          P: StaticProtocol<EpollEvent, H>
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
            d: PhantomData {},
        })
    }
}

impl<H, P> Server for Mux<H, P>
    where H: Handler<EpollEvent>,
          P: StaticProtocol<EpollEvent, H>
{
    fn get_epoll_config(&self) -> EpollConfig {
        self.epoll_config
    }

    fn stop(&mut self) {
        self.terminated = true;
    }

    fn bind(self, mask: SigSet) -> Result<()> {
        trace!("bind()");

        try!(eintr!(bind, "bind", self.srvfd, &self.sockaddr));
        info!("bind: fd {} to {}", self.srvfd, self.sockaddr);

        try!(eintr!(listen, "listen", self.srvfd, self.max_conn));
        info!("listen: fd {} with max connections: {}",
              self.srvfd,
              self.max_conn);

        let ceinfo = EpollEvent {
            events: EPOLLIN | EPOLLET | EPOLLEXCLUSIVE | EPOLLWAKEUP,
            data: self.srvfd as u64,
        };

        try!(self.cepfd.register(self.srvfd, &ceinfo));
        trace!("registered main thread's interest on {}", self.srvfd);

        let io_threads = self.io_threads;
        let epoll_config = self.epoll_config;
        let srvfd = self.srvfd;
        let protocol = self.protocol;

        for i in 1..io_threads {

            let epfd = EpollFd::new(try!(epoll_create()));

            try!(epfd.register(srvfd, &ceinfo));
            trace!("registered thread's {} interest on {}", i, srvfd);


            thread::spawn(move || {
                trace!("spawned new thread {}", i);
                // add the set of signals to the signal mask for all threads
                mask.thread_block().unwrap();
                let mut epoll = Epoll::from_fd(epfd,
                                               protocol.get_handler(From::from(0_usize), epfd, i),
                                               epoll_config);

                let aff = i % ::num_cpus::get();
                let mut cpuset = sched::CpuSet::new();
                cpuset.set(aff).unwrap();

                sched::sched_setaffinity(0, &cpuset).unwrap();

                debug!("set thread's {} affinity to cpu {}", i, aff);

                info!("starting I/O thread's {} event loop", i);
                epoll.run();
            });
        }

        let mut epoll = Epoll::from_fd(self.cepfd,
                                       protocol.get_handler(From::from(0_usize), self.cepfd, 0),
                                       epoll_config);

        debug!("created {} I/O epoll instances", self.io_threads);
        info!("starting I/O thread's 0 event loop");

        let aff = 0;
        let mut cpuset = sched::CpuSet::new();
        cpuset.set(aff).unwrap();

        sched::sched_setaffinity(0, &cpuset).unwrap();

        debug!("set thread's 0 affinity to cpu {}", aff);

        epoll.run();
        Ok(())
    }
}


impl<H, P> Drop for Mux<H, P>
    where H: Handler<EpollEvent>,
          P: StaticProtocol<EpollEvent, H>
{
    fn drop(&mut self) {
        let _ = unistd::close(self.srvfd).unwrap();
    }
}
