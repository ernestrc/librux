use std::net::ToSocketAddrs;
use std::os::unix::io::RawFd;

use nix::sys::socket::*;
use nix::sys::signalfd::SigSet;
use nix::sys::signal;
use nix::unistd;
use nix::sched;

use error::*;
use poll::*;
use protocol::*;
use prop::Prop;

/// Prop implementation that creates one AF_INET/SOCK_STREAM socket and uses it to bind/listen
/// at the specified address. It will create one handler/epoll instance per `io_thread` plus one for the main thread.
pub struct Server {
    srvfd: RawFd,
    cepfd: EpollFd,
    sockaddr: SockAddr,
    max_conn: usize,
    io_threads: usize,
    children: Vec<i32>,
    epoll_config: EpollConfig,
}

pub struct ServerConfig {
    max_conn: usize,
    io_threads: usize,
    sockaddr: SockAddr,
    epoll_config: EpollConfig,
}

impl ServerConfig {
    pub fn new<A: ToSocketAddrs>(addr: A) -> Result<ServerConfig> {
        let inet = try!(addr.to_socket_addrs().unwrap().next().ok_or("could not parse sockaddr"));
        let sockaddr = SockAddr::Inet(InetAddr::from_std(&inet));

        let cpus = ::num_cpus::get();
        let max_conn = 5000 * cpus;
        let io_threads = if cpus > 2 {
            cpus - 1 //1 for logging/signals
        } else {
            1
        };

        Ok(ServerConfig {
            sockaddr: sockaddr,
            max_conn: max_conn,
            io_threads: io_threads,
            epoll_config: Default::default(),
        })
    }

    pub fn max_conn(self, max_conn: usize) -> ServerConfig {
        ServerConfig { max_conn: max_conn, ..self }
    }

    pub fn io_threads(self, io_threads: usize) -> ServerConfig {
        assert!(io_threads > 0, "I/O threads must be greater than 0");
        ServerConfig { io_threads: io_threads, ..self }
    }

    pub fn epoll_config(self, epoll_config: EpollConfig) -> ServerConfig {
        ServerConfig { epoll_config: epoll_config, ..self }
    }
}

impl Server {
    pub fn new(config: ServerConfig) -> Result<Server> {

        let ServerConfig { io_threads, max_conn, sockaddr, epoll_config } = config;

        // create connections epoll
        let fd = try!(epoll_create());

        let cepfd = EpollFd { fd: fd };

        // create socket
        let srvfd = try!(socket(AddressFamily::Inet, SockType::Stream, SOCK_NONBLOCK, 0)) as i32;

        setsockopt(srvfd, sockopt::ReuseAddr, &true).unwrap();

        Ok(Server {
            sockaddr: sockaddr,
            cepfd: cepfd,
            srvfd: srvfd,
            children: Vec::with_capacity(io_threads),
            max_conn: max_conn,
            io_threads: io_threads,
            epoll_config: epoll_config,
        })
    }

    fn shutdown(&self) {
        debug!("{:?} killing child I/O processes: {:?}",
               unistd::getpid(),
               &self.children);
        for child in self.children.iter() {
            debug!("killing child {:?}", child);
            signal::kill(*child, signal::Signal::SIGKILL).unwrap();
        }
        unistd::close(self.srvfd).unwrap();
    }
}

impl Prop for Server {
    fn get_epoll_config(&self) -> EpollConfig {
        self.epoll_config
    }

    fn setup<'p, P>(&mut self, mask: SigSet, protocol: &'p P) -> Result<Epoll<P::H>>
        where P: StaticProtocol<'p, EpollEvent, EpollCmd>
    {
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
        trace!("registered main process interest on {}", self.srvfd);

        let io_threads = self.io_threads;
        let epoll_config = self.epoll_config;
        let srvfd = self.srvfd;

        for i in 1..io_threads {

            let epfd = EpollFd::new(try!(epoll_create()));

            try!(epfd.register(srvfd, &ceinfo));
            trace!("registered process {} interest on {}", i, srvfd);


            match unistd::fork() {
                Ok(unistd::ForkResult::Parent { child, .. }) => {
                    debug!("{:?} created I/O child process n {} with pid {}",
                           unistd::getpid(),
                           i,
                           child);
                    self.children.push(child);
                    trace!("{:?} children: {:?}", unistd::getpid(), self.children);
                    continue;
                }
                Ok(unistd::ForkResult::Child) => {
                    // add the set of signals to the signal mask for all threads
                    mask.thread_block().unwrap();
                    let handler = protocol.get_handler(Position::Root, epfd, i);
                    let mut epoll = Epoll::from_fd(epfd, handler, epoll_config);

                    let aff = i % ::num_cpus::get();
                    let mut cpuset = sched::CpuSet::new();
                    cpuset.set(aff).unwrap();

                    sched::sched_setaffinity(0, &cpuset).unwrap();

                    debug!("set process {} affinity to cpu {}", i, aff);

                    info!("starting I/O process {} event loop", i);
                    epoll.run();
                    return Err(format!("stopped event loop of {}", i).into());
                }
                Err(e) => panic!("fork(): {}", e),
            }
        }

        let epoll = Epoll::from_fd(self.cepfd,
                                   protocol.get_handler(Position::Root, self.cepfd, 0),
                                   epoll_config);

        debug!("created {} I/O epoll instances", self.io_threads);
        info!("starting I/O process 0 event loop");

        let aff = 0;
        let mut cpuset = sched::CpuSet::new();
        cpuset.set(aff).unwrap();

        sched::sched_setaffinity(0, &cpuset).unwrap();

        debug!("set process 0 affinity to cpu {}", aff);

        Ok(epoll)
    }

    fn stop(&self) {
        self.shutdown();
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        // self.shutdown();
    }
}
