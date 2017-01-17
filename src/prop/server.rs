use ::RawFd;
use epoll::*;
use error::*;
use handler::Handler;
use mux::*;
use nix::sched;
use nix::sys::signalfd::SigSet;
use nix::sys::socket::*;
use prop::Prop;
use std::net;
use std::net::ToSocketAddrs;
use std::thread;

pub struct Server<H> {
  srvfd: RawFd,
  epfd: EpollFd,
  sockaddr: SockAddr,
  max_conn: usize,
  handler: H,
  io_threads: usize,
  epoll_config: EpollConfig,
}

pub struct ServerConfig {
  max_conn: usize,
  io_threads: usize,
  socktype: SockType,
  sockaddr: SockAddr,
  sockflag: SockFlag,
  sockproto: i32,
  family: AddressFamily,
  epoll_config: EpollConfig,
}

// TODO: provide optional socket based activation
// http://man7.org/linux/man-pages/man7/daemon.7.html
// http://man7.org/linux/man-pages/man3/sd_listen_fds.3.html
// http://man7.org/linux/man-pages/man5/systemd.socket.5.html
impl ServerConfig {

  pub fn tcp<A: ToSocketAddrs>(addr: A) -> Result<ServerConfig> {
    let (sockaddr, family) = Self::inet(addr)?;
    ServerConfig::new(sockaddr, SockType::Stream, family, SOCK_NONBLOCK, 6)
  }

  pub fn udp<A: ToSocketAddrs>(addr: A) -> Result<ServerConfig> {
    let (sockaddr, family) = Self::inet(addr)?;
    ServerConfig::new(sockaddr, SockType::Datagram, family, SOCK_NONBLOCK, 17)
  }

  pub fn new(sockaddr: SockAddr, socktype: SockType, family: AddressFamily, sockflag: SockFlag,
             sockproto: i32)
             -> Result<ServerConfig> {

    let cpus = ::num_cpus::get();
    let max_conn = 5000 * cpus;
    let io_threads = ::std::cmp::max(1, cpus - 1);

    Ok(ServerConfig {
      sockaddr: sockaddr,
      socktype: socktype,
      sockflag: sockflag,
      sockproto: sockproto,
      family: family,
      max_conn: max_conn,
      io_threads: io_threads,
      epoll_config: Default::default(),
    })
  }

  pub fn max_conn(self, max_conn: usize) -> ServerConfig {
    ServerConfig { max_conn: max_conn, ..self }
  }

  pub fn sockflag(self, sockflag: SockFlag) -> ServerConfig {
    ServerConfig { sockflag: sockflag, ..self }
  }

  pub fn io_threads(self, io_threads: usize) -> ServerConfig {
    assert!(io_threads > 0, "I/O threads must be greater than 0");
    ServerConfig { io_threads: io_threads, ..self }
  }

  pub fn epoll_config(self, epoll_config: EpollConfig) -> ServerConfig {
    ServerConfig { epoll_config: epoll_config, ..self }
  }

  fn inet<A: ToSocketAddrs>(addr: A) -> Result<(SockAddr, AddressFamily)> {
    let inet_addr_std = addr.to_socket_addrs()
      .unwrap()
      .next()
      .ok_or("could not parse sockaddr")?;
    let inet_addr = InetAddr::from_std(&inet_addr_std);
    let sockaddr = SockAddr::Inet(inet_addr);

    let family = match inet_addr_std {
      net::SocketAddr::V4(_) => AddressFamily::Inet,
      net::SocketAddr::V6(_) => AddressFamily::Inet6,
    };

    Ok((sockaddr, family))
  }
}

impl<H> Server<H> {
  pub fn new_with<F>(config: ServerConfig, new_handler: F) -> Result<Server<H>>
    where F: FnOnce(EpollFd) -> H,
  {

    let ServerConfig { io_threads,
                       max_conn,
                       socktype,
                       sockaddr,
                       sockflag,
                       sockproto,
                       family,
                       epoll_config } = config;

    let fd = epoll_create()?;

    let epfd = EpollFd { fd: fd };

    let srvfd = socket(family, socktype, sockflag, sockproto)? as i32;

    setsockopt(srvfd, sockopt::ReuseAddr, &true).unwrap();

    Ok(Server {
      sockaddr: sockaddr,
      epfd: epfd,
      srvfd: srvfd,
      handler: new_handler(epfd),
      max_conn: max_conn,
      io_threads: io_threads,
      epoll_config: epoll_config,
    })
  }
}

impl<'m, H, F, R> Server<SyncMux<'m, H, F, R>>
  where H: Handler<MuxEvent<'m, R>, MuxCmd> + EpollHandler,
        F: HandlerFactory<'m, H, R> + 'm,
        R: Clone + 'm,
{
  pub fn new(config: ServerConfig, factory: F) -> Result<Server<SyncMux<'m, H, F, R>>> {

    let max_conn = config.max_conn;
    let server = Server::new_with(config, |epfd| {
      SyncMux::new(max_conn, epfd, factory)
    })?;

    Ok(server)
  }
}

impl<H> Prop for Server<H>
  where H: Handler<EpollEvent, EpollCmd> + EpollHandler + Send + Clone + 'static,
{
  type EpollHandler = H;

  // TODO reload configuration
  fn reload(&mut self) {}

  fn setup(&mut self, mask: SigSet) -> Result<Epoll<Self::EpollHandler>> {

    syscall!(bind(self.srvfd, &self.sockaddr))?;
    info!("bind: fd {} to {}", self.srvfd, self.sockaddr);

    syscall!(listen(self.srvfd, self.max_conn))?;
    info!("listen: fd {} with max connections: {}", self.srvfd, self.max_conn);

    let ceinfo = EpollEvent {
      events: H::interests(),
      data: self.srvfd as u64,
    };

    self.epfd.register(self.srvfd, &ceinfo)?;
    debug!("registered main interest on {}", self.srvfd);

    let io_threads = self.io_threads;
    let epoll_config = self.epoll_config;
    let srvfd = self.srvfd;

    for i in 1..io_threads {

      let epfd = EpollFd::new(epoll_create()?);

      epfd.register(srvfd, &ceinfo)?;
      debug!("registered thread {} interest on {}", i, srvfd);

      let mut handler = self.handler.clone();

      handler.with_epfd(epfd);

      thread::spawn(move || {
        // add the set of signals to the signal mask for all threads
        mask.thread_block().unwrap();

        let mut epoll = Epoll::from_fd(epfd, handler, epoll_config);

        let aff = i % ::num_cpus::get();
        let mut cpuset = sched::CpuSet::new();
        cpuset.set(aff).unwrap();

        // TODO optional
        sched::sched_setaffinity(0, &cpuset).unwrap();

        debug!("set thread {} affinity to cpu {}", i, aff);

        info!("starting I/O thread {} event loop", i);

        epoll.run();
      });
    }

    let epoll = Epoll::from_fd(self.epfd, self.handler.clone(), epoll_config);

    debug!("created {} I/O epoll instances", self.io_threads);
    info!("starting I/O thread 0 event loop");

    let aff = 0;
    let mut cpuset = sched::CpuSet::new();
    cpuset.set(aff).unwrap();

    sched::sched_setaffinity(0, &cpuset).unwrap();

    debug!("set thread 0 affinity to cpu {}", aff);

    Ok(epoll)
  }
}
