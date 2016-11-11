#![feature(test)]

extern crate test;
extern crate rux;

use rux::buf::ByteBuffer;
use test::Bencher;

const SIZE: usize = 1024 * 1024 * 1024;
const CHUNK: usize = 1024 * 1024;

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
    buf.write(&[1; SIZE]).unwrap();
    b.iter(|| {
        let mut slice = [0; CHUNK];
        let cnt = buf.read(&mut slice).unwrap();
        buf.consume(cnt);
    });
}

#[bench]
pub fn bench_bytebuffer_rw_chunks(b: &mut Bencher) {
    let mut buf = ByteBuffer::with_capacity(SIZE);
    b.iter(|| {
        let slice = [1; CHUNK];
        let mut slice2 = [0; CHUNK];
        {
            buf.write(&slice).unwrap();
        }
        {
            let cnt = buf.read(&mut slice2).unwrap();
            buf.consume(cnt);
        }
    });
}
