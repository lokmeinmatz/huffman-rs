use crate::binary_io::BinaryReader;
use crate::{Node, HEADER};
use std::fs::File;
use std::io::{self, BufWriter, Read, Write};
use std::path::PathBuf;

/// branch: 0
/// leaf:   1
fn construct_tree(reader: &mut BinaryReader<File>) -> io::Result<Node> {
    let is_leaf = reader.read_bit()?;
    //dbg!(&is_leaf);
    if is_leaf {
        let value = reader.read_byte()?;

        Ok(Node::Leaf(0, value))
    } else {
        let left = construct_tree(reader)?;
        let right = construct_tree(reader)?;

        Ok(Node::Branch(0, Box::new(left), Box::new(right)))
    }
}

fn traverse_tree(reader: &mut BinaryReader<File>, node: &Node) -> io::Result<u8> {
    match node {
        Node::Branch(_, l, r) => {
            let go_right = reader.read_bit()?;
            if go_right {
                traverse_tree(reader, r)
            } else {
                traverse_tree(reader, l)
            }
        }
        Node::Leaf(_, v) => Ok(*v),
    }
}

pub fn decode(path: PathBuf) -> io::Result<()> {
    let mut file = std::fs::File::open(&path)?;

    // check if header is correct
    let h_len = HEADER.len();
    let mut h_buf = vec![0; h_len];

    file.read_exact(&mut h_buf[..])?;

    // check if header is valid
    if HEADER != &h_buf[..] {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Header of file is not valid",
        ));
    }


    let mut reader = BinaryReader::new(file);

    let root: Node = construct_tree(&mut reader)?;
    let mut path_new = path.clone();
    // get file name

    match path.extension() {
        Some(e) => {
            if e.to_str().unwrap().ends_with(".huff") {
                let mut new_ext = e.to_str().unwrap().to_owned();

                new_ext.truncate(new_ext.len() - 5);

                path_new.set_extension(new_ext);
            } else {
                path_new.set_extension("txt");
            }
        }
        None => {
            path_new.set_extension("txt");
        }
    }
    if cfg!(debug_assertions) {
        println!("{:#?}", root);
    }

    println!("Creating file @ {:?}", path_new);
    let mut writer = BufWriter::new(File::create(path_new)?);
    let mut bytes_written = 0;
    while let Ok(val) = traverse_tree(&mut reader, &root) {
        bytes_written += 1;

        if bytes_written % 10_000 == 0 {
            println!("kb written: {}", bytes_written / 1000);
        }
        if val == 0x1c {
            break;
        }
        writer.write_all(&[val])?;
    }

    Ok(())
}
