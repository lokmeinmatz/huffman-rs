use std::fs::File;
use std::io::{self, BufReader, BufWriter, Read, Write};

pub struct BinaryWriter {
    pub buf_writer: BufWriter<File>,
    bit_buf: usize,
    bits_written: u8,
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
            bit_buf: 0,
            bits_written: 0,
            bytes_written: 0
        }
    }

    pub fn get_bytes_written(&self) -> usize {
        self.bytes_written
    }

    pub fn write_buf(&mut self) -> io::Result<()> {
        let arr = self.bit_buf.to_be_bytes();
        /* unsafe {
            std::mem::transmute(self.bit_buf.to_be())
        }; */
        if cfg!(debug_assertions) && HIGH_DEBUG {
            println!("write out: {:#066b} arr: {:x?}", self.bit_buf, arr);
        }
        self.buf_writer.write(&arr)?;

        self.bytes_written += MAX_BIT_BUF_BYTES;

        self.bits_written = 0;
        self.bit_buf = 0;
        Ok(())
    }

    pub fn write_bit(&mut self, b: bool) -> io::Result<()> {
        // write bit

        //before
        if cfg!(debug_assertions) && HIGH_DEBUG {
            println!("before bit: {:#066b}", self.bit_buf);
        }

        if b {
            self.bit_buf |= 1 << (MAX_BIT_BUF_BYTES as u8 * 8 - 1 - self.bits_written);
            // 1 << (MAX_BIT_BUF_BYTES - 1) : last bit is 1
        }

        //after
        if cfg!(debug_assertions) && HIGH_DEBUG {
            println!("after bit:  {:#066b}", self.bit_buf);
        }

        self.bits_written += 1;

        //println!("bit  {:#034b}", self.bit_buf);
        if self.bits_written >= MAX_BIT_BUF_BYTES as u8 * 8 {
            self.write_buf()?;
        }

        Ok(())
    }

    pub fn write_byte(&mut self, b: u8) -> io::Result<()> {
        let bits_for_buf = std::cmp::min(8, (MAX_BIT_BUF_BYTES as u8 * 8) - self.bits_written);
        if cfg!(debug_assertions) && HIGH_DEBUG {
            println!("bfb: {} buf: {:#066b}", bits_for_buf, self.bit_buf);
        }
        if bits_for_buf == 8 {
            self.bit_buf |= (b as usize) << (MAX_BIT_BUF_BYTES as u8 * 8 - self.bits_written - 8);
            self.bits_written += 8;

            if self.bits_written == MAX_BIT_BUF_BYTES as u8 * 8 {
                self.write_buf()?;
            }
        } else {
            //writes the bits that fit into buf
            self.bit_buf |= (b as usize) >> (8 - bits_for_buf);

            self.write_buf()?;

            // write remaining bits
            let rem_bits = 8 - bits_for_buf;

            self.bit_buf = (b as usize) << MAX_BIT_BUF_BYTES as u8 * 8 - rem_bits;
            self.bits_written = rem_bits;
        }
        //println!("byte {:#034b}", self.bit_buf);
        Ok(())
    }

    pub fn write_path(&mut self, path: &Vec<bool>) -> io::Result<()> {
        for b in path {
            self.write_bit(*b)?;
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

        self.bit_buf = self.bit_buf << 1;
        self.bits_read += 1;
        return res;
    }

    pub fn read_byte(&mut self) -> io::Result<u8> {
        if self.bits_read >= MAX_BIT_BUF_BYTES as u8 * 8 {
            self.read_buf()?;
        }
        if self.bits_read <= MAX_BIT_BUF_BYTES as u8 * 8 - 8 {
            let res = Ok((self.bit_buf >> (MAX_BIT_BUF_BYTES * 8 - 8)) as u8);
            //println!("{:#034b}", self.bit_buf);
            self.bits_read += 8;
            self.bit_buf = self.bit_buf << 8;

            return res;
        } else {
            // problem: part of this byte is stored in next chunk
            let remaining = 8 - (MAX_BIT_BUF_BYTES as u8 * 8 - self.bits_read);

            assert!(remaining <= 8);

            let mut res = (self.bit_buf >> MAX_BIT_BUF_BYTES * 8 - 8) as u8; // stores the high bits at the right place;
            self.read_buf()?;

            res |= (self.bit_buf >> (MAX_BIT_BUF_BYTES as u8 * 8 - remaining)) as u8;

            self.bit_buf = self.bit_buf << remaining;
            self.bits_read += remaining;

            return Ok(res);
        }
    }
}

impl Drop for BinaryWriter {
    fn drop(&mut self) {
        println!("Writing remaining bits...");
        let mut x = self.bit_buf;
        let mut rem_bits: Vec<u8> = Vec::with_capacity(self.bits_written as usize / 8);
        while self.bits_written > 0 {
            rem_bits.push((x >> (MAX_BIT_BUF_BYTES * 8 - 8)) as u8); // take the left most byte and push
            x = x << 8; // move bytes to left 1 byte
            self.bits_written = self.bits_written.saturating_sub(8);
            self.bytes_written += 1;
        }

        self.buf_writer.write(&rem_bits).unwrap();
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
