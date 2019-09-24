use crate::binary_io::BinaryWriter;
use crate::{Node, HEADER, MAX_BUF_SIZE};
use std::collections::HashMap;
use std::ffi::OsStr;
use std::io::{self, Read, Write};
use std::path::PathBuf;

fn add_to_lookup(lookup: &mut HashMap<u8, Vec<bool>>, parent_path: Vec<bool>, node: &Node) {
    match node {
        Node::Branch(_, l1, l2) => {
            let mut l1_path = parent_path.clone();
            l1_path.push(false);
            add_to_lookup(lookup, l1_path, l1);
            let mut l2_path = parent_path.clone();
            l2_path.push(true);
            add_to_lookup(lookup, l2_path, l2);
        }
        Node::Leaf(_, byte) => {
            lookup.insert(*byte, parent_path);
        }
    }
}

/// Writes Leaves as 1 bit followed by byte of the value
/// Branch starts with 0 bit followed by left and right node
fn write_tree(node: &Node, out: &mut BinaryWriter) -> io::Result<()> {
    match node {
        Node::Leaf(_, b) => {
            out.write_bit(true)?;
            //println!("Written leaf");
            out.write_byte(*b)?;
        }
        Node::Branch(_, l, r) => {
            out.write_bit(false)?;
            //println!("Written branch");

            write_tree(l, out)?;
            write_tree(r, out)?;
        }
    }

    Ok(())
}

pub fn encode(path: PathBuf) -> io::Result<()> {
    let mut file = std::fs::File::open(&path)?;


    struct Statistics {
        read_bytes: usize,
        written_bytes: usize
    };

    let mut stats = Statistics {
        read_bytes: 0,
        written_bytes: 0
    };

    let mut buf: [u8; MAX_BUF_SIZE] = [0; MAX_BUF_SIZE];
    let mut counter: [usize; 256] = [0; 256];

    while let Ok(bytes_read) = file.read(&mut buf) {

        stats.read_bytes += bytes_read;

        if bytes_read == 0 {
            break;
        }

        for i in 0..bytes_read {
            let byte = buf[i];

            counter[byte as usize] += 1;
        }
    }

    counter[0x1c] = 1;

    // create boxed nodes
    let mut tree: Vec<Box<Node>> = Vec::new();

    for b in 0..256 {
        let c = counter[b];
        if c > 0 {
            // occurs at least once
            tree.push(Box::new(Node::Leaf(c, b as u8)));
        }
    }

    // TODO: Add ending node to indecate that te file is ended to tree
    //       Maybe as u9 where last bit is only set if is end so the tree contains and end node

    while tree.len() > 1 {
        let mut lowest_two = (1usize, 0usize);
        for i in 0..tree.len() {
            let count = tree[i].count();
            if count < tree[lowest_two.1].count() {
                lowest_two = (lowest_two.0, i);
            } else if count < tree[lowest_two.0].count() && lowest_two.1 != i {
                lowest_two = (i, lowest_two.1);
            }
        }

        //println!("len: {} lowest: {:?}", tree.len(), lowest_two);

        if lowest_two.0 < lowest_two.1 {
            lowest_two = (lowest_two.1, lowest_two.0);
        }

        // now we got the lowest two
        let combined_count = tree[lowest_two.0].count() + tree[lowest_two.1].count();

        // move them out of the vec
        let l1 = tree.remove(lowest_two.0);
        let l2 = tree.remove(lowest_two.1);

        let branch = Node::Branch(combined_count, l1, l2);
        tree.push(Box::new(branch));
    }

    let root = tree.remove(0);

    if cfg!(debug_assertions) {
        println!("{:#?}", root);
    }

    // now create a lookup table
    let mut lookup: HashMap<u8, Vec<bool>> = HashMap::new();

    add_to_lookup(&mut lookup, Vec::new(), &root);

    println!("Created Lookup table, starting encoding...");

    file = std::fs::File::open(&path)?;
    let mut out_path = path.clone();

    let extension = match path.extension() {
        Some(e) => {
            let mut e = e.to_os_string();
            e.push(OsStr::new(".huff"));
            e
        }
        None => OsStr::new("huff").to_os_string(),
    };

    out_path.set_extension(extension);

    println!("File will be saved @ {:?}", &out_path);

    let mut writer = BinaryWriter::new(std::fs::File::create(&out_path)?);

    // write header
    println!("Writing header");
    writer
        .buf_writer
        .write(HEADER)
        .map_err(|_e| io::Error::new(io::ErrorKind::Other, "Error while writing header"))?;

    // write tree

    // for each node, if is leaf, write 1 and 8 bit value
    //                if is branch, write 0 and recursively write node
    println!("Writing tree");
    write_tree(&root, &mut writer)?;

    println!("Writing data");
    while let Ok(bytes_read) = file.read(&mut buf) {
        if bytes_read == 0 {
            break;
        }

        for i in 0..bytes_read {
            let byte = buf[i];

            // get path to byte
            match lookup.get(&byte) {
                Some(path_vec) => writer.write_path(path_vec)?,
                None => {
                    return Err(io::Error::new(
                        io::ErrorKind::NotFound,
                        "Byte not in lookup-table",
                    ))
                }
            }
        }
    }

    match lookup.get(&0x1c) {
        Some(path_vec) => writer.write_path(path_vec)?,
        None => {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                "FileSeperator Byte not in lookup-table",
            ))
        }
    }

    stats.written_bytes = writer.get_bytes_written();

    println!(" --- Stats ---\nBytes read: {}\nBytes written: {}\nCompression rate: {}%\n", stats.read_bytes, stats.written_bytes, (stats.read_bytes as f64 / stats.written_bytes as f64) * 100.0);

    Ok(())
}
