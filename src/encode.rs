use crate::binary_io::BinaryWriter;
use crate::{Node, HEADER, MAX_BUF_SIZE};
use bitvec::prelude::*;
use std::collections::HashMap;
use std::ffi::OsStr;
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicU16, Ordering},
    mpsc::{channel, sync_channel, Receiver},
    Arc, Mutex,
};
use std::thread::{self, JoinHandle};

fn add_to_lookup(lookup: &mut HashMap<u8, BitVec>, parent_path: BitVec, node: &Node) {
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

struct PreData {
    id: usize,
    len: usize,
    content: Box<[u8]>,
}

struct PostData {
    id: usize,
    content: BitVec,
}

pub fn encode(path: PathBuf) -> io::Result<()> {
    let mut file = std::fs::File::open(&path)?;

    struct Statistics {
        read_bytes: usize,
        written_bytes: usize,
    };

    let mut stats = Statistics {
        read_bytes: 0,
        written_bytes: 0,
    };

    // calculate how many threads are needed

    let thread_count = std::cmp::min(
        crate::MAX_WORKERS - 1,
        file.metadata().expect("File metadata error").len() as usize / MAX_BUF_SIZE,
    ) + 1;

    println!("Worker threads used: {}", thread_count);

    let mut r_buf: Vec<u8> = vec![0; MAX_BUF_SIZE];
    let mut counter: [usize; 256] = [0; 256];

    while let Ok(bytes_read) = file.read(&mut r_buf) {
        stats.read_bytes += bytes_read;

        if bytes_read == 0 {
            break;
        }

        for i in 0..bytes_read {
            let byte = r_buf[i];

            counter[byte as usize] += 1;
        }
    }

    // the end byte
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
    std::mem::drop(tree);

    if cfg!(debug_assertions) {
        //println!("{:#?}", root);
    }

    // now create a lookup table
    let mut lookup: HashMap<u8, BitVec> = HashMap::new();

    add_to_lookup(&mut lookup, BitVec::new(), &root);

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

    let mut workers: Vec<JoinHandle<()>> = Vec::with_capacity(thread_count);
    let lookup = Arc::new(lookup);
    let (pre_sender, pre_receiver) = sync_channel::<Option<PreData>>(4);
    let (post_sender, post_receiver) = channel::<PostData>();
    let workers_active = Arc::new(AtomicU16::new(0));
    let postdata_waiting = Arc::new(AtomicU16::new(0));

    let feed: Arc<Mutex<Receiver<Option<PreData>>>> = Arc::new(Mutex::new(pre_receiver));

    for t_id in 0..thread_count {
        let feed = feed.clone();
        let lookup = lookup.clone();
        let post_sender = post_sender.clone();
        let workers_active = workers_active.clone();
        let postdata_waiting = postdata_waiting.clone();

        // tell worker "will" be active
        workers_active.fetch_add(1, Ordering::Relaxed);

        workers.push(thread::Builder::new().name(format!("worker_{}", t_id)).spawn(move || {
            println!("thread {} waiting", t_id);
            loop {

                let data = {
                    match feed.lock().expect("Feed Mutex poisoned").recv().unwrap() {
                        Some(data) => data,
                        None => break
                    }
                };

                println!("[w{}] received {} bytes", t_id, data.len);
                let mut compressed: BitVec<BigEndian, u8> = BitVec::with_capacity(64);

                for i in 0..data.len {
                    let byte = data.content[i];
                    // get path to byte
                    match lookup.get(&byte) {
                        Some(path_vec) => compressed.extend(path_vec),
                        None => {
                            panic!("Byte not in lookup-table");
                        }
                    }
                }

                println!("[w{}] sends {} bits", t_id, compressed.len());
                // send data to writer thread
                postdata_waiting.fetch_add(1, Ordering::Relaxed);
                post_sender.send(PostData {
                    id: data.id,
                    content: compressed
                }).expect("Could not send PostData");
            }
            println!("[w{}] finished", t_id);
            workers_active.fetch_sub(1, Ordering::Relaxed);
        }).unwrap());
    }

    println!("Workers active: {}", workers_active.load(Ordering::Relaxed));

    // writer thread
    let writer_thread = thread::Builder::new().name("writer".to_owned()).spawn(move || {
        println!("Writer-thread running");

        let mut buf: Vec<PostData> = Vec::new();
        let mut next_expected: usize = 0;

        while workers_active.load(Ordering::Relaxed) > 0 || postdata_waiting.load(Ordering::Relaxed) > 0  {
            let p_dat = post_receiver.recv().unwrap();
            postdata_waiting.fetch_sub(1, Ordering::Relaxed);
            println!("writer received {} bits", p_dat.content.len());
            // case 1: p_dat ist next expected package
            if p_dat.id == next_expected {
                writer.write_path(&p_dat.content).unwrap();
                next_expected += 1;

                let mut n_idx = 0;

                while n_idx < buf.len() && buf[n_idx].id == next_expected {
                    writer.write_path(&p_dat.content).unwrap();
                    next_expected += 1;
                    n_idx += 1;
                }

                if n_idx > 0 {
                    buf.drain(0..n_idx);
                }
            }

            // case 2: p_dat is somewhere in buf
            else if buf.len() > 0 && p_dat.id < buf.last().unwrap().id {
                // find where to insert
                let mut pos = 0;
                while pos < buf.len() {
                    if buf[pos].id > p_dat.id {
                        break;
                    }
                    pos += 1;
                }

                buf.insert(pos, p_dat);
            }
            // case 3: p_dat is at the end
            else {
                buf.push(p_dat);
            }
        }

        if buf.len() > 0 {
            eprintln!("Still got {} unproccessed packages", buf.len());
            eprintln!("First unprocessed id: {}", buf[0].id);
            panic!("Not all packets processed");
        }

        // add finish byte
        match lookup.get(&0x1c) {
            Some(path_vec) => writer.write_path(path_vec).unwrap(),
            None => {
                panic!("FileSeperator Byte not in lookup-table");
            }
        }

        stats.written_bytes = writer.get_bytes_written();

        println!(
            " --- Stats ---\nBytes read: {}\nBytes written: {}\nCompression rate: {}%\n",
            stats.read_bytes,
            stats.written_bytes,
            (stats.read_bytes as f64 / stats.written_bytes as f64) * 100.0
        );
    }).unwrap();


    let mut pre_id = 0;
    while let Ok(bytes_read) = file.read(&mut r_buf) {
        if bytes_read == 0 {
            break;
        }
        println!("Sending {} bytes for proceccing", bytes_read);
        // fill queue
        pre_sender.send(Some(PreData {
            id: pre_id,
            len: bytes_read,
            content: r_buf.clone().into_boxed_slice()
        })).expect("Sending PreData failed");

        pre_id += 1;
    }

    // terminate workers
    for _ in &workers {
        pre_sender.send(None).unwrap();
    }

    for t in workers {
        t.join().unwrap();
    }

    writer_thread.join().unwrap();    

    Ok(())
}
