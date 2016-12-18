use poll::EpollFd;

pub mod mux;

pub trait Handler {
    type In;
    type Out;

    #[inline]
    fn ready(&mut self, Self::In) -> Self::Out;
}

pub trait Root
    where Self: Handler,
{
    #[inline]
    fn update(&mut self, epfd: EpollFd);
}
