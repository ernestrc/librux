#![feature(test)]

extern crate test;
extern crate rux;

use rux::buf::ByteBuffer;
use test::Bencher;

const SIZE: usize = ::std::u32::MAX as usize;
const CHUNK: usize = 16;

#[bench]
pub fn bench_bytebuffer_write_chunks(b: &mut Bencher) {
    let mut buf = ByteBuffer::with_capacity(SIZE);
    b.iter(|| {
        buf.write(&[1; CHUNK]).unwrap();
    })
}


#[bench]
pub fn bench_bytebuffer_read_chunks(b: &mut Bencher) {
    let mut buf = ByteBuffer::with_capacity(SIZE);
    b.iter(|| {
        let cnt = buf.read(&mut [0; CHUNK]).unwrap();
        buf.consume(cnt);
    });
}

#[bench]
pub fn bench_bytebuffer_rw_chunks(b: &mut Bencher) {
    let mut buf = ByteBuffer::with_capacity(SIZE);
    b.iter(|| {
        {
            buf.write(&[1; CHUNK]).unwrap();
        }
        {
            let cnt = buf.read(&mut [0; CHUNK]).unwrap();
            buf.consume(cnt);
        }
    });
}
