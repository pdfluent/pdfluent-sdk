use std::io::Read;

use cff_parser::{Table, string_by_id};

fn main() {
    // open the file passed on the comanmand line
    let path = std::env::args().nth(1).expect("no file given");
    let file = std::fs::File::open(&path).expect("could not open file");
    let mut reader = std::io::BufReader::new(file);
    let mut buffer = Vec::new();
    reader.read_to_end(&mut buffer).expect("could not read file");
    let table = Table::parse(&buffer).unwrap();

    let encoding = table.encoding.get_code_to_sid_table(&table.charset);
    for (cid, sid) in encoding.iter() {
        println!("{}: {:?}", cid, string_by_id(&table, *sid));
    }

}