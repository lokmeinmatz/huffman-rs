use std::fs::File;
use std::io::{self, BufReader, BufWriter, Read, Write};
use bitvec::prelude::*;

const MAX_WRITER_BITCAP : usize = 512 * 8;

pub struct BinaryWriter {
    pub buf_writer: BufWriter<File>,
    bit_buf: BitVec<BigEndian, u8>,
    bytes_written: usize
}

pub struct BinaryReader {
    buf_reader: BufReader<File>,
    bit_buf: usize,
    bits_read: u8,
}

const MAX_BIT_BUF_BYTES: usize = std::mem::size_of::<usize>();
const HIGH_DEBUG: bool = false;

impl BinaryWriter {
    pub fn new(f: File) -> Self {
        dbg!(MAX_BIT_BUF_BYTES);
        BinaryWriter {
            buf_writer: BufWriter::new(f),
            bit_buf: BitVec::with_capacity(MAX_WRITER_BITCAP),
            bytes_written: 0
        }
    }

    pub fn get_bytes_written(&self) -> usize {
        self.bytes_written
    }

    pub fn write_buf(&mut self) -> io::Result<()> {
        // write bytes that are "ready", copy last "not ready" byte to new bit_buf

        let bytes_ready = self.bit_buf.len() / 8;
        if bytes_ready == 0 {return Ok(())}

        // check if last byte is ready
        let bits_rem = self.bit_buf.len() - (bytes_ready * 8);
        if bits_rem == 0 {
            let slice = self.bit_buf.as_slice();
            // can write all
            self.buf_writer.write_all(&slice)?;
            self.bit_buf.clear();
        }
        else {
            // need to save last byte
            let slice = self.bit_buf.as_slice();
            self.buf_writer.write_all(&slice[..(slice.len() - 1)])?;
            self.bit_buf = BitVec::from_element(*slice.last().unwrap());
            self.bit_buf.truncate(bits_rem);
            self.bit_buf.reserve(MAX_WRITER_BITCAP - 1);
        }

        self.bytes_written += bytes_ready;

        Ok(())
    }

    pub fn write_bit(&mut self, b: bool) -> io::Result<()> {
        // write bit

        self.bit_buf.push(b);

        if self.bit_buf.len() > MAX_WRITER_BITCAP {
            self.write_buf()?;
        }

        Ok(())
    }

    pub fn write_byte(&mut self, b: u8) -> io::Result<()> {
        self.bit_buf.extend(b.as_bitslice::<BigEndian>());

        if self.bit_buf.len() > MAX_WRITER_BITCAP {
            self.write_buf()?;
        }
        Ok(())
    }

    pub fn write_path(&mut self, path: &BitVec) -> io::Result<()> {

        // TODO instead of bitwise writing, write as much bytes as possible directly
        // and only store the remaining bits in an buffer?
        self.bit_buf.extend(path);

        if self.bit_buf.len() > MAX_WRITER_BITCAP {
            self.write_buf()?;
        }
        Ok(())
    }
}

impl BinaryReader {
    pub fn new(f: File) -> Self {
        BinaryReader {
            buf_reader: BufReader::new(f),
            bit_buf: 0,
            bits_read: MAX_BIT_BUF_BYTES as u8 * 8, // force read_buf when first read
        }
    }

    pub fn read_buf(&mut self) -> io::Result<()> {
        let mut tbuf = [0u8; MAX_BIT_BUF_BYTES];

        let read_bytes = self.buf_reader.read(&mut tbuf)?;

        //println!("{}", read_bytes);
        if read_bytes == 0 {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "Reached end"));
        }

        //TODO: read less than sizeof<usize> bytes
        //println!("{:?}", tbuf);
        let val = usize::from_be_bytes(tbuf);

        if cfg!(debug_assertions) && HIGH_DEBUG {
            println!("read buf {:#066b}", val);
        }

        self.bit_buf = val;
        self.bits_read = 0;

        Ok(())
    }

    pub fn read_bit(&mut self) -> io::Result<bool> {
        if self.bits_read >= MAX_BIT_BUF_BYTES as u8 * 8 {
            self.read_buf()?;
        }
        let res = Ok((self.bit_buf >> (MAX_BIT_BUF_BYTES * 8 - 1)) != 0);

        self.bit_buf <<= 1;
        self.bits_read += 1;
        res
    }

    pub fn read_byte(&mut self) -> io::Result<u8> {
        if self.bits_read >= MAX_BIT_BUF_BYTES as u8 * 8 {
            self.read_buf()?;
        }
        if self.bits_read <= MAX_BIT_BUF_BYTES as u8 * 8 - 8 {
            let res = Ok((self.bit_buf >> (MAX_BIT_BUF_BYTES * 8 - 8)) as u8);
            //println!("{:#034b}", self.bit_buf);
            self.bits_read += 8;
            self.bit_buf <<= 8;

            res

        } else {
            // problem: part of this byte is stored in next chunk
            let remaining = 8 - (MAX_BIT_BUF_BYTES as u8 * 8 - self.bits_read);

            assert!(remaining <= 8);

            let mut res = (self.bit_buf >> (MAX_BIT_BUF_BYTES * 8 - 8)) as u8; // stores the high bits at the right place;
            self.read_buf()?;

            res |= (self.bit_buf >> (MAX_BIT_BUF_BYTES as u8 * 8 - remaining)) as u8;

            self.bit_buf <<= remaining;
            self.bits_read += remaining;

            Ok(res)
        }
    }
}

impl Drop for BinaryWriter {
    fn drop(&mut self) {
        println!("Writing remaining bits...");
        

        self.write_buf().unwrap();
    }
}

#[test]
fn binary_io_test() -> Result<(), io::Error> {
    {
        let mut writer: BinaryWriter = BinaryWriter::new(std::fs::File::create("./test.bin")?);
        writer.write_bit(true)?;
        for i in 1..10 {
            writer.write_byte(i)?;
            writer.write_bit(i % 2 == 0)?;
            println!("{} {}", i, i % 2 == 0);
        }
    }

    //          1|0000000  1|0|000000  10|1|00000  011|0|0000  0100
    //
    // content: true 1 false 2 true 3 false 4 true 5 false 6 true 7 false 8 true 9 false

    {
        let mut reader: BinaryReader = BinaryReader::new(std::fs::File::open("./test.bin")?);

        assert_eq!(reader.read_bit()?, true);

        println!("reading loop");
        for i in 1..10 {
            assert_eq!(reader.read_byte()?, i);
            assert_eq!(reader.read_bit()?, i % 2 == 0);
        }
    }

    Ok(())
}
