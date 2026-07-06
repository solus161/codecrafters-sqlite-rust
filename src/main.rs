use anyhow::{Result, bail};
use crate::btree::BTree;
use crate::pager::Pager;

#[macro_use]
mod utils;
mod exceptions;
mod parser;
mod pager;
mod btree;
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
        },
        ".tables" => {
            let mut table_names: Vec<&str> = Vec::new();
            for t in btree.tables.values() {
                table_names.push(t.name());
            };
            table_names.sort();
            let names = table_names.join(" ");
            println!("{}", &names);
        }
        _ => {
            let output = process(&mut btree, command.as_str());
            match output {
                Ok(v) => println!("{}", &v),
                Err(e) => println!("ERR {:?}", &e)
            }
        }
    }

    Ok(())
}
