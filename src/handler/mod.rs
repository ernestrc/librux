use poll::EpollFd;

pub mod mux;

pub trait Handler {
    type In;
    type Out;

    #[inline]
    fn ready(&mut self, Self::In) -> Self::Out;
}

pub trait Reset
    where Self: Handler,
{
    #[inline]
    fn reset(&mut self, epfd: EpollFd);
}
