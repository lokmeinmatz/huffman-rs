#![feature(box_syntax)]

use std::env;
use std::io::{self};
use std::path::PathBuf;

mod binary_io;

mod encode;
use encode::encode;

mod decode;
use decode::decode;

#[derive(Debug)]
enum MainMode {
    Encoding,
    Decoding
}


fn main() -> io::Result<()> {
    let args : Vec<String> = env::args().skip(1).collect();

    let (mode, file_path) : (MainMode, String) = if let Some(e) = args.iter().find(|a| a == &"--encode" || a == &"-e") {
        match args.iter().find(|a| a != &e) {
            Some(path) => {(MainMode::Encoding, path.to_owned())},
            None => return Err(io::Error::new(io::ErrorKind::InvalidInput, "No path specified"))
        }
        
    } 

    else if let Some(e) = args.iter().find(|a| a == &"--decode" || a == &"-d") {
        match args.iter().find(|a| a != &e) {
            Some(path) => {(MainMode::Decoding, path.to_owned())},
            None => return Err(io::Error::new(io::ErrorKind::InvalidInput, "No path specified"))
        }
    } else {
        eprintln!("You didn't specify wheather to decode or encode the data. Guessing based on file ending");
        match args.iter().next() {
            Some(path) => {
                if path.ends_with(".huff") {
                    (MainMode::Decoding, path.to_owned())
                }
                else {
                    (MainMode::Encoding, path.to_owned())
                }
                },
            None => return Err(io::Error::new(io::ErrorKind::InvalidInput, "No path specified"))
        }
    };

    let path = PathBuf::from(file_path);

    if ! path.exists() {
        return Err(io::Error::new(io::ErrorKind::NotFound, "File doesn't exist."))
    }

    println!("{:?} {:?}", mode, &path);

    let start = std::time::Instant::now();

    match mode {
        MainMode::Encoding => {
            encode(path)?
        },
        MainMode::Decoding => {
            decode(path)?
        }
    }

    let dur = start.elapsed();
    println!("Finished. Took {}s {}ms", dur.as_secs(), dur.subsec_millis());
    Ok(())
}


#[derive(Debug)]
pub enum Node {
    Branch(usize, Box<Node>, Box<Node>),
    Leaf(usize, u8)
}

impl Node {
    pub fn count(&self) -> usize {
        match self {
            Node::Branch(count, _, _) => *count,
            Node::Leaf(count, _) => *count
        }
    }
}

pub const MAX_BUF_SIZE : usize = 1024 * 128;
pub const MAX_WORKERS : usize = 4;
pub const HEADER : &[u8] = b"HUFFMAN 0.1 Matthias Kind";