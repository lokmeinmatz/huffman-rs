use bitvec::prelude::*;
use std::io::{self, Read, Write};


const MAX_WRITER_BITCAP: usize = 128 * 128;
const MAX_READER_BITCAP: usize = 128;

pub struct BinaryWriter<T: Write> {
    pub writer: T,
    bit_buf: BitVec<BigEndian, u8>,
    bytes_written: usize,
}

pub struct BinaryReader<T: Read> {
    reader: T,
    bit_buf: BitVec<BigEndian, u8>
}

impl<T: Write> BinaryWriter<T> {
    pub fn new(w: T) -> Self {
        BinaryWriter {
            writer: w,
            bit_buf: BitVec::with_capacity(MAX_WRITER_BITCAP),
            bytes_written: 0,
        }
    }

    pub fn inner(mut self) -> T {
        self.finish();
        self.writer
    }

    pub fn get_bytes_written(&self) -> usize {
        self.bytes_written
    }

    pub fn write_buf(&mut self) -> io::Result<()> {
        // write bytes that are "ready", copy last "not ready" byte to new bit_buf

        let bytes_ready = self.bit_buf.len() / 8;
        if bytes_ready == 0 {
            return Ok(());
        }

        // check if last byte is ready
        let bits_rem = self.bit_buf.len() - (bytes_ready * 8);
        if bits_rem == 0 {
            let slice = self.bit_buf.as_slice();
            // can write all
            self.writer.write_all(&slice)?;
            self.bit_buf.clear();
        } else {
            // need to save last byte
            let slice = self.bit_buf.as_slice();
            self.writer.write_all(&slice[..(slice.len() - 1)])?;
            self.bit_buf = BitVec::from_element(*slice.last().unwrap());
            self.bit_buf.truncate(bits_rem);
            self.bit_buf.reserve(MAX_WRITER_BITCAP - 1);
        }
        self.writer.flush()?;
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

    pub fn write_path(&mut self, path: &BitSlice) -> io::Result<()> {
        // TODO instead of bitwise writing, write as much bytes as possible directly
        // and only store the remaining bits in an buffer?
        self.bit_buf.extend(path);

        if self.bit_buf.len() > MAX_WRITER_BITCAP {
            self.write_buf()?;
        }
        Ok(())
    }

    pub fn finish(&mut self) {
        println!("Writing remaining bits...");

        self.writer.write_all(&self.bit_buf.as_slice()[0..self.bit_buf.len() / 8]).unwrap();
        self.writer.flush().unwrap();
    }
}

impl<T: Read> BinaryReader<T> {
    pub fn new(r: T) -> Self {
        BinaryReader {
            reader: r,
            bit_buf: BitVec::new()
        }
    }

    /// Reads from reader MAX_READER_BITCAP / 8 bytes. 
    /// Overwrites the buffer! so make sure to save data you need to yous later
    fn read_buf(&mut self) -> io::Result<()> {
        // allocate new space
        self.bit_buf.resize(MAX_READER_BITCAP, false);
        //println!("BinaryReader::read_buf()");
        let read_bytes = self.reader.read(self.bit_buf.as_mut_slice())?;
        assert!(read_bytes * 8 <= self.bit_buf.len());
        
        self.bit_buf.resize(read_bytes * 8, false);
        //println!("c:{} | {}", read_bytes, self.bit_buf);
        if read_bytes == 0 {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "Reached end"));
        }

        Ok(())
    }

    pub fn read_bit(&mut self) -> io::Result<bool> {
        if self.bit_buf.len() == 0 {
            self.read_buf()?;
            if self.bit_buf.len() == 0 {
                // no more bits
                return Err(io::Error::new(io::ErrorKind::Other, "No more bits can be read."));
            }
        }
        Ok(self.bit_buf.remove(0))
    }

    pub fn read_byte(&mut self) -> io::Result<u8> {
        if self.bit_buf.is_empty() {
            self.read_buf()?;
        }
        let mut res = self.bit_buf.as_slice()[0];
        if self.bit_buf.len() < 8 {
            let old_bits = self.bit_buf.len();
            res &= 0xff << (8 - old_bits);
            println!("{:b} keep {} bits", res, old_bits);
            self.read_buf()?;

            // copy
            let new_res = self.bit_buf.as_slice()[0];
            res |= new_res >> old_bits;
            println!("new_res: {:b} res: {:b}", new_res, res);

            self.bit_buf.drain(0..(8 - old_bits));

            if self.bit_buf.len() < 8 {
                return Err(io::Error::new(io::ErrorKind::Other, "Not enough bits read."));
            }
            return Ok(res);
        }

        // copy from bit 8 to bitbuf
        self.bit_buf = self.bit_buf.split_off(8);

        Ok(res)
    }
}


#[test]
fn binary_io_test() -> Result<(), io::Error> {
    use std::fs::File;
    //let mut storage: Vec<u8> = vec![0; 512];
    {
        //let mut writer: BinaryWriter<&mut [u8]> =
        //    BinaryWriter::new(storage.as_mut_slice());
        let mut writer: BinaryWriter<File> =
            BinaryWriter::new(File::create("bin_test.bin")?);
        writer.write_bit(true)?;
        for i in 1..1000 {
            writer.write_byte((i % 256)  as u8)?;
            writer.write_bit(i % 2 == 0)?;
            //println!("{} {}", i, i % 2 == 0);
            //println!("{} {:b}", writer.bit_buf, i);
        }

        writer.finish();
        //f = writer.inner();
    }

    //          1|0000000  1|0|000000  10|1|00000  011|0|0000  0100
    //
    // content: true 1 false 2 true 3 false 4 true 5 false 6 true 7 false 8 true 9 false

    //println!("{:?}", &storage[0..20]);
    {
        //let mut reader: BinaryReader<&[u8]> = BinaryReader::new(&storage);
        let mut reader: BinaryReader<File> = BinaryReader::new(File::open("bin_test.bin")?);

        assert_eq!(reader.read_bit()?, true);

        println!("reading loop");
        for i in 1..1000 {
            println!("loop {}", i);
            assert_eq!(reader.read_byte()?, (i % 256) as u8);
            assert_eq!(reader.read_bit()?, i % 2 == 0);
        }
    }

    Ok(())
}
