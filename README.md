# huffman-rs
## About
A huffman encoder/decoder for learning, can compress the bible by 150%...

## Usage
For testing, run `cargo run -- bible.txt` for encoding the bible to the compressed format, and then `cargo run -- bible.txt.huff` for decoding.
You can also override the auto-detection (which is based on the file ending) by passing `--encode`/`-e` or `--decode`/`-d` to the args.

** Building in Release-Mode gives about 10-15 times speedup!**

## How it works
### Encoding
1. The program first scans the whole file and counts how often each byte occurs
2. Then a [Huffman tree](https://en.wikipedia.org/wiki/Huffman_coding) is generated
3. The tree gets written in binary format to the output file.
4. Each leave of the tree gets added to a HashMap for faster Lookup, and the path to it is the key
5. The whole input file gets read again, and for each byte the matching entry of the lookup table gets written to the output file.

### Decoding
1. The program reads the tree from the file and reconstructs the internal representation
2. bit for bit the reader traverses the tree, until it finds a leaf node with an byte to write or the EOF-node

### Bitwise Read/Write
For bitwise reading and writing there are Wrappers around the BufWriter/BufReader in binary_io.rs
They read one usize a time and buffer it themself, and read the next if not enough bits are remaining.

## TODO for the future
- speed up the writing process (now takes about 60% just to finish the writing process)
- parallize the decoder