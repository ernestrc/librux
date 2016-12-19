use poll::EpollFd;

pub mod mux;

pub trait Handler<In, Out> {
    #[inline]
    fn ready(&mut self, In) -> Out;
}

pub trait Reset {
    #[inline]
    fn reset(&mut self, epfd: EpollFd);
}

impl<T, In, Out> Handler<In, Out> for T
    where T: FnMut(In) -> Out,
{
    fn ready(&mut self, event: In) -> Out {
        self(event)
    }
}
