#![allow(dead_code, unused_imports, unused_variables)]

use crate::btree::BTree;
use crate::pager::Pager;
use anyhow::{Result, bail};

#[macro_use]
mod utils;
mod btree;
mod exceptions;
mod pager;
mod parser;
mod processor;

use processor::process;

fn main() -> Result<()> {
    // Parse arguments
    let args = std::env::args().collect::<Vec<_>>();
    match args.len() {
        0 | 1 => bail!("Missing <database path> and <command>"),
        2 => bail!("Missing <command>"),
        _ => {}
    }

    // Init pager and btree
    let pager = Pager::new(&args[1]);
    let mut btree = BTree::new(pager);
    let _ = btree.parse_meta();

    // Parse command and act accordingly
    let command = &args[2];
    match command.as_str() {
        ".dbinfo" => {
            // You can use print statements as follows for debugging, they'll be visible when running tests.
            eprintln!("Logs from your program will appear here!");

            // TODO: Uncomment the code below to pass the first stage
            println!("database page size: {}", btree.pager.page_size());
            println!("number of tables: {}", btree.tables.len());
        }
        ".tables" => {
            let mut table_names: Vec<&str> = Vec::new();
            for t in btree.tables.values() {
                table_names.push(t.name());
            }
            table_names.sort();
            let names = table_names.join(" ");
            println!("{}", names);
        }
        // Support extra args for db exploration
        ".page" => {
            let page_no = &args[3].parse::<u64>().expect("Page no must be int");
            let page = btree.parse_page(page_no).expect("Failed parsing page");
            println!("{:?}", page);
        }
        _ => {
            let output = process(&mut btree, command.as_str());
            match output {
                Ok(v) => println!("{}", v),
                Err(e) => println!("ERR {:?}", e),
            }
        }
    }

    Ok(())
}
