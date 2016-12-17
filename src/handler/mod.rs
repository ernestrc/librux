use poll::EpollFd;

pub mod mux;

pub trait Handler {
    type In;
    type Out;

    #[inline]
    fn update(&mut self, epfd: EpollFd);

    #[inline]
    fn ready(&mut self, Self::In) -> Self::Out;
}
