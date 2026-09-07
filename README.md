[![progress-banner](https://backend.codecrafters.io/progress/sqlite/6ff5f10f-58c3-47d5-acac-f5afed1c3e31)](https://app.codecrafters.io/users/solus161?r=2qF)

This is a starting point for Rust solutions to the
["Build Your Own SQLite" Challenge](https://codecrafters.io/challenges/sqlite).

In this challenge, you'll build a barebones SQLite implementation that supports
basic SQL queries like `SELECT`. Along the way we'll learn about
[SQLite's file format](https://www.sqlite.org/fileformat.html), how indexed data
is
[stored in B-trees](https://jvns.ca/blog/2014/10/02/how-does-sqlite-work-part-2-btrees/)
and more.

**Note**: If you're viewing this repo on GitHub, head over to
[codecrafters.io](https://codecrafters.io) to try the challenge.

# Passing the first stage

The entry point for your SQLite implementation is in `src/main.rs`. Study and
uncomment the relevant code, and then run the command below to execute the tests
on our servers:

```sh
codecrafters submit
```

Time to move on to the next stage!

# Stage 2 & beyond

Note: This section is for stages 2 and beyond.

1. Ensure you have `cargo (1.95)` installed locally
1. Run `./your_program.sh` to run your program, which is implemented in
   `src/main.rs`. This command compiles your Rust project, so it might be slow
   the first time you run it. Subsequent runs will be fast.
1. Run `codecrafters submit` to submit your solution to CodeCrafters. Test
   output will be streamed to your terminal.

# Sample Databases

To make it easy to test queries locally, we've added a sample database in the
root of this repository: `sample.db`.

This contains two tables: `apples` & `oranges`. You can use this to test your
implementation for the first 6 stages.

You can explore this database by running queries against it like this:

```sh
$ sqlite3 sample.db "select id, name from apples"
1|Granny Smith
2|Fuji
3|Honeycrisp
4|Golden Delicious
```

There are two other databases that you can use:

1. `superheroes.db`:
   - This is a small version of the test database used in the table-scan stage.
   - It contains one table: `superheroes`.
   - It is ~1MB in size.
1. `companies.db`:
   - This is a small version of the test database used in the index-scan stage.
   - It contains one table: `companies`, and one index: `idx_companies_country`
   - It is ~7MB in size.

These aren't included in the repository because they're large in size. You can
download them by running this script:

```sh
./download_sample_databases.sh
```

If the script doesn't work for some reason, you can download the databases
directly from
[codecrafters-io/sample-sqlite-databases](https://github.com/codecrafters-io/sample-sqlite-databases).

---

# My implementation

A read-only SQLite engine written from scratch in Rust: it opens a real `.db`
file, walks the on-disk B-trees itself, and answers a subset of `SELECT` with
its own tokenizer, parser and query executor. No SQL or SQLite library is used —
only `anyhow`/`thiserror` for errors and `paste` for builder macros.

## How to use

```sh
# build + run (release build, cached under /tmp)
./your_program.sh <database path> "<command>"
```

Dot-commands:

```sh
$ ./your_program.sh sample.db .dbinfo
database page size: 4096
number of tables: 3

$ ./your_program.sh sample.db .tables
apples oranges sqlite_sequence
```

SQL:

```sh
$ ./your_program.sh sample.db "select count(*) from apples"
4

$ ./your_program.sh sample.db "select id, name from apples where color = 'Yellow'"
4|Golden Delicious

# uses idx_companies_country instead of scanning ~7MB
$ ./your_program.sh companies.db "select id, name from companies where country = 'oman'"
```

Run the test suite (needs `companies.db` downloaded):

```sh
cargo test
```

## What's implemented

- **File format**: database header parsing (page size, page count), page reader
  with the 100-byte header offset on page 1.
- **Schema**: `sqlite_schema` (page 1) is read and every `CREATE TABLE` /
  `CREATE INDEX` statement stored in it is parsed back into an in-memory
  catalogue of tables, their column order, and the indexes attached to them.
- **B-tree traversal**: all four page types — table interior/leaf and index
  interior/leaf — including **overflow page chains** for payloads larger than a
  page.
- **Record decoding**: varints, the serial-type header, and the value types
  (NULL, 8/16/24/32/48/64-bit ints, float, 0/1 constants, blob, text), with
  `rowid` handled as the implicit `id` column.
- **SQL front-end**: a hand-written tokenizer and recursive-descent AST builder
  covering `SELECT`, `CREATE TABLE` and `CREATE INDEX`.
- **Queries**:
  - `select <cols> | * | count(*) from <table>`
  - `where` with `=`, `!=`, `>`, `<`, `>=`, `<=`, joined by `AND`/`OR` and
    nested with parentheses
  - quoted identifiers (`"size range"`), string/int/float literals
- **Two execution strategies**: full table scan, or an index scan when a `WHERE`
  condition matches an existing index — the planner picks automatically.
- **Errors**: one `CustomErr` enum (`thiserror`) across tokenize → build AST →
  validate → execute.

Not implemented: writes, joins, `ORDER BY`/`GROUP BY`, aggregates other than
`count(*)`, `LIKE`/`IN`/`BETWEEN`/`NULL` predicates (tokenized but not
evaluated), and multi-column indexes.

## Implementation highlights

**Layered by concern.** `pager.rs` (bytes → pages) → `btree.rs` (pages → cells,
records, catalogue) → `parser/` (SQL → AST) → `processor.rs` (AST + B-tree →
rows). Each layer only knows about the one below it.

- `src/pager.rs` — the only thing that touches the file. Positional reads
  (`read_exact_at`), so no seek state to keep in sync. Handles the page-size
  quirk where the stored value `1` means 65536.
- `src/btree.rs` — page/cell parsing plus the schema catalogue. Uses the
  builder pattern (`PageBuilder`, `TableBuilder`, `IndexBuilder`) with
  `paste`-generated setters in `src/utils.rs` so a half-built page can never
  escape as a `Page`.
- `src/parser/tokenizer.rs` — tokens borrow (`&'a str`) from the source SQL, so
  tokenizing allocates almost nothing; the AST then owns its strings and outlives
  the input.
- `src/parser/select.rs` — the `WHERE` tree is built with an expression stack and
  an operator stack, so `AND` binds tighter than `OR` and parentheses recurse
  into a nested `WhereExpr`. Two traits keep it uniform: `Build` (tokens → node)
  and `Eval` (node + row → bool).
- **Late binding of columns.** The parser only records column *names*; after the
  table is known, `stmt.resolve(&table)` rewrites each `ColumnExpr` with its
  ordinal in the record. Evaluating a row is then an index lookup, not a string
  compare.
- **Index selection.** `resolve_index()` walks the `WHERE` tree looking for a
  condition usable as an index probe — descending only through `AND` branches,
  since an `OR` branch can't restrict the scan. If the column has an index, the
  executor walks the index tree, collects `rowid`s, then fetches just those rows.
- **Index descent is comparison-aware.** Because index keys order on the tuple
  `(indexed_value, rowid)`, an equality run can be promoted across several
  separators in an interior page. `index_scan` handles each of `=`, `>`, `>=`,
  `<`, `<=` separately so the walk doesn't stop early on duplicates or bleed
  past the run.
- **Row fetch caches pages.** `extract_rows` memoises parsed pages in a
  `HashMap` while resolving a batch of `rowid`s, so the upper levels of the
  table tree are parsed once instead of once per row.

## Deepdive: how SQLite stores data
### Basic Principle
SQLite db files are just .. file, but consist of "pages". Pages have fized size, so a page could be easily located/read given an offset number (how many pages come before that needed page). A page can have any size from 512 up to 65536, a 2^n.

This concept of page is different from the one in OS operations relating to RAM and disk swapping. Typically, a read/write operation will operate in a block of 4096 bytes to balance between efficienty and fragmentation. So it is better to have SQLite page size in 4096 bytes.

Each page is assigned a number, from 1 to 2^23 - 2.

#### First page, the header and `sqlite_schema` table
File header takes up to first 100 bytes. Given the `sample.db` from Codecrafters repo, we got this:
```
00000000: 5351 4c69 7465 2066 6f72 6d61 7420 3300  SQLite format 3.
# Fist 16 bytes, magic string

00000010: 1000 0101 0040 2020 0000 0419 0000 0776  .....@  .......v
#         4096 -> page size, big endian,
#              01 -> file format write version, 2 for WAL
#                01 -> file format read version, 2 for WAL
#                                       0000 1910 -> nbr of pages,
#                                       488960 lines on nvim, 
#                                       488960*16/4096 = 1910 pages

00000020: 0000 0000 0000 0000 0000 0002 0000 0004  ................
00000030: 0000 0000 0000 0000 0000 0001 0000 0000  ................
#                             0000 0001 -> utf-8

00000040: 0000 0000 0000 0000 0000 0000 0000 0000  ................
00000050: 0000 0000 0000 0000 0000 0000 0000 0005  ................
00000060: 002e 4b90 0d00 0000 030e c300 0f8f 0f3d  ..K............=
#    header ends->| |<-sqlite_schema table starts
00000070: 0ec3 0000 0000 0000 0000 0000 0000 0000  ................

```

#### Pages and B-tree Page Types

Both tables and indexes are b-tree. And b-tree/page must start with a header. First byte of header indicates page type:
- `0x02`: interior index b-tree page
- `0x05`: interior table b-tree page
- `0x0a`: leaf index b-tree page
- `0x0d`: leaf table b-tree page

The following bytes in page header having fixed size for ease of parsing:
- Offset 1, size 2: start of first freeblock on the page, could be zero;
- Offset 3, size 2: nbr of cells in the page
- Offset 5, size 2: start of cell content area, 0 for 65536
- Offset 7, size 1: number of fragmented free bytes within the cell content area
- Offset 8, size 4: right most pointer, only appear for interior b-tree page

Take a look at `sqlite_schema` table at page 1:
```
00000060: 002e 43c3 0d00 0000 030e 4d00 0f06 0eb4  ..C.......M.....
#                   || -> leaf table b-tree page
#                     || -> zero free block
#                          |---| -> 3 cells in this page 
#                                -> this table has 3 objects:
#                                   - table companies, sqlite_sequence
#                                   - index idx_companies_country
#                               |---| -> 3661 start of cell content
#                                    || -> zero fragmented free bytes, end of 12-byte header
#                                       |-----| -> cell array of 3 * 2 bytes
#                                       for interior b-tree page these 4 bytes are for right-most pointer
#                                       3846 -> offset start of 1st cell, smallest key
#                                            3764 -> offset start of 2nd cell

00000070: 0e4d 0000 0000 0000 0000 0000 0000 0000  .M..............
#         3661 -> offset start of 3rd cell, largest key
```

We could notice that cells having larger key is located nearer from the start of page. But that is just coincidence. In reality there could be noncontiguous unallocated space. This will be defined by freeblock. In this example, the space between the last cell pointerin cell pointer array (3661) and the next first cell (also 3661) is unallocated.

Follow offset 3846 (from the start of the page), we got first cell
```
00000f00: 652c 7365 7129 8177 0107 171f 1f01 833d  e,seq).w.......=
#                        |-> first cell starts with .w

00000f10: 7461 626c 6563 6f6d 7061 6e69 6573 636f  tablecompaniesco
00000f20: 6d70 616e 6965 7302 4352 4541 5445 2054  mpanies.CREATE T
00000f30: 4142 4c45 2063 6f6d 7061 6e69 6573 0a28  ABLE companies.(
00000f40: 0a09 6964 2069 6e74 6567 6572 2070 7269  ..id integer pri
00000f50: 6d61 7279 206b 6579 2061 7574 6f69 6e63  mary key autoinc
00000f60: 7265 6d65 6e74 0a2c 206e 616d 6520 7465  rement., name te
00000f70: 7874 2c20 646f 6d61 696e 2074 6578 742c  xt, domain text,
00000f80: 2079 6561 725f 666f 756e 6465 6420 7465   year_founded te
00000f90: 7874 2c20 696e 6475 7374 7279 2074 6578  xt, industry tex
00000fa0: 742c 2022 7369 7a65 2072 616e 6765 2220  t, "size range" 
00000fb0: 7465 7874 2c20 6c6f 6361 6c69 7479 2074  text, locality t
00000fc0: 6578 742c 2063 6f75 6e74 7279 2074 6578  ext, country tex
00000fd0: 742c 2063 7572 7265 6e74 5f65 6d70 6c6f  t, current_emplo
00000fe0: 7965 6573 2074 6578 742c 2074 6f74 616c  yees text, total
00000ff0: 5f65 6d70 6c6f 7965 6573 2074 6578 7429  _employees text)
00001000: 0500 0000 040f e100 0000 066c 0ff9 0ff1  ...........l....
```

That's the content of a cell, and that depends on page types.

#### Cell On Page
The layout of cell content depends on page type:
| Type       | Tbl Lf 0x0d | Tbl Int 0x05 | Idx Lf 0x0a | Idx Int 0x02 | Desc                            |
|:-----------|-------------|----------------|-------------|----------------|---------------------------------|
| 4-byte int |             | x              |             | x              | Page no of left child           |
| varint     | x           |                | x           | x              | Payload len (bytes)             |
| varint     | x           | x              |             |                | Row id                          |
| byte array | x           |                | x           | x              | Payload                          |
| 4-byte int | x           |                | x           | x              | Page no of first overflow page  |

**Notes**:
- The 4 bytes for page pointer of first overflow page could be ommitted if all payload fit the btree page

##### VarInt
A varint (variable-length integer) is SQLite's way of encoding integers using as few bytes as possible. Instead of always using a fixed 4 or 8 bytes, small numbers take 1 byte, and only genuinely large numbers take more — up to 9 bytes max.
SQLite's varint encoding rule:
- Each byte uses its high bit as a "continuation flag": 1 means "more bytes follow," 0 means "this is the last byte."
- The remaining 7 bits of each byte hold actual data.
- Bytes are read most-significant first (big-endian-ish, but 7 bits at a time).
- Special case: the 9th byte (if you get that far) uses all 8 bits, not 7 — this caps the encoding at 9 bytes for a full 64-bit value.

Let's examine the sample db file.
```
00000f00: 652c 7365 7129 81       77     0107 171f 1f01 833d  e,seq).w.......=
#                        10000001 01110111
#                        1 -> read first byte in varint
#                                 0 -> last byte in varint 
#                        -> 1111 0111 -> payload length 247 = 7 for header and 240 for body, we'll see later
#                                        00000001 -> rowId = 1

```

##### Record Format
Payload follows a specific format:
- A header: specify types of each column encoded as serial type varint
- A body: value of each column

| Serial Type | Content Size | Meaning  |
|:---:|:---:|:---|
| 0 | 0 |  Value is a NULL.  |
| 1 | 1 |  Value is an 8-bit twos-complement integer.  |
| 2 | 2 |  Value is a big-endian 16-bit twos-complement integer.  |
| 3 | 3 |  Value is a big-endian 24-bit twos-complement integer.  |
| 4 | 4 |  Value is a big-endian 32-bit twos-complement integer.  |
| 5 | 6 |  Value is a big-endian 48-bit twos-complement integer.  |
| 6 | 8 |  Value is a big-endian 64-bit twos-complement integer.  |
| 7 | 8 |  Value is a big-endian IEEE 754-2008 64-bit floating point number.  |
| 8 | 0 |  Value is the integer 0. (Only available for schema format 4 and higher.)  |
| 9 | 0 |  Value is the integer 1. (Only available for schema format 4 and higher.)  |
| 10,11      | variable | Reserved for internal use. |
| N≥12 and even      | (N-12)/2 |  Value is a BLOB that is (N-12)/2 bytes in length.  |
| N≥13 and odd      | (N-13)/2 |  Value is a string in the text encoding and (N-13)/2 bytes in length. |
```
00000f00: 652c 7365 7129 8177 0107 171f 1f01 833d  e,seq).w.......=
#                               || -> header 7 bytes length, include this varint
#                                  23 -> string length 5
#                                    31 -> string length 9
#                                       31 -> string length 9
#                                         01 -> i8
#                                            445 -> string length 216 
```

Compare this header with the body, we got:
```
table -> 5 bytes string, column "type"
companies -> 9 bytes string, column "name"
companies -> 9 bytes string, column "tbl_name"
. -> 0x02 -> 2 int, column "rootpage"
CREATE TABLE companies.(..id integer primary key .. -> 216 bytes string, column "sql"
```

Additionally, we don't find the metadata for `sqlite_schema` table anywhere in page 1, because the column names is hard coded.

So far, we have achieved:
- Read the `sqlite_schema`
- Follow cell pointer array to cell in the same page

Next, we'll try to navigate other objects: table `companies`, and index `idx_companies_country`.

### Query the table `companies`
Table `companies` is fist at root page 2, byte offset 4096.

```
00001000: 0500 0000 040f e100 0000 066c 0ff9 0ff1  ...........l....
#         || 0x05 -> table interior page
#           |---| zero freeblock
#                |---| -> 4 cells on page
#                     |---| -> 4065 start of cell content
#                          || -> no fragmented free byte
#                             |-------| -> 1644 right-most pointer
#                                       |--| -> cell at 4089, smallest key
#                                            |--| -> cell at 4081
00001010: 0fe9 0fe1 0000 0000 0000 0000 0000 0000  ................
#         |--| -> cell at 4073
#              |--| -> cell at 4065, largest key
```

Let's check these cells
```
00001fe0: 0000 0005 0482 f9f0 1400 0003 6a82 a8cd  ............j...
#           |--------| -> cell offset 4065, 1284 left child pointer
#                     |--------| -> varint 6,191,124, row id
#                               |--------| -> cell offset 4073, 874 left child pointer
#                                         |------
00001ff0: 2c00 0001 ce81 cb87 0300 0001 cdeb 8502  ,...............
#         -| -> vartin 4,859,564, row id
#           |--------| -> cell offset 4081, 462 left child pointer
#                     |--------| -> varint 3,326,852, row id
#                               |--------| -> cell offset 4089, 461 left child pointer
#                                         |-----| -> varint 1,753,730 row id
00002000: 0d00 0000 010f ef00 0fef 0000 0000 0000  ................
00002010: 0000 0000 0000 0000 0000 0000 0000 0000  ................
```

Again we see a pattern emerged:
- Cells are appended from the end of page, upto beginning of free content area
- The row id/key is also in increasing order, from end of page

To get get a row with target row id/key, we need to traverse the b-tree:
- From table root page, check for cells with key >= target
- Visit that left child pointer, check again .. till you got to leaf page
- If all cells in current interior page having key < target, visit the right-most pointer page to check again

Let's try to traverse the tree to get to a leaf page, given left child pointer 461:
- Line offset (461 -1)*4096/16 = 117760, still a table interior page
```
001cc000: 0500 0001 8e05 2100 0000 0193 0ffa 0ff4  ......!.........
#         || -> table interior page
#           |---| -> zero freeblock
#                |---| -> 398 cells on page
#                                       |--| -> cell at 4090


001ccff0: 0007 e746 0000 0006 c509 0000 0005 a264  ...F...........d
#                                  |-------| -> 5 left child pointer
001cd000: 0500 0001 6e04 e100 0000 0304 0ff9 0ff2  ....n...........
```
- Line offset (5 - 1)*4096/16 = 1024, nice we got to a leaf page
```
00004000: 0d00 0000 2200 9500 0f7a 0f34 0ea8 0e38  ...."....z.4...8
#         || -> table leaf page
#           |---| -> zero freeblock
#                |---| -> 366 cells on page
#                     |---| -> 149 start of cell content area
#                          || -> no fragmented freeblock
#                             |--| -> cell at 3962

00004f70: 6775 6572 6e73 6579 3131 8102 811f 0b00  guernsey11......
#                                  |--| -> varint 130 payload length
#                                       |--| -> varint 159 row id
#                                            || -> header length 11
#                                              || -> 0 NULL value
00004f80: 452d 192f 1d55 1511 1167 6c6f 6261 6c20  E-./.U...global 
#         || -> varint 69, string length (69 - 13)/2 = 28
#           || -> varint 45, string length (45 - 13)/2 = 16
#              || -> varint 25, string length (25 - 13)/2 = 6
#                || -> varint 47, string length (47 - 13)/2 = 17
#                   || -> varint 29, string length (29 - 13)/2 = 8
#                     || -> varint 85, string length (85 - 13)/2 = 36
#                        || -> varint 21, string length (21 - 13)/2 = 4
#                          || -> varint 17, string length (17 - 13)/2 = 2
#                             || -> varint 17, string length (17 - 13)/2 = 2
00004f90: 636f 6d70 7574 6572 2073 6572 7669 6365  computer service
00004fa0: 7320 6c6c 6367 6c6f 6263 6f6d 2d6f 6d61  s llcglobcom-oma
00004fb0: 6e2e 636f 6d31 3939 382e 3063 6f6d 7075  n.com1998.0compu
00004fc0: 7465 7220 736f 6674 7761 7265 3531 202d  ter software51 -
00004fd0: 2032 3030 6275 726e 7376 696c 6c65 2c20   200burnsville, 
00004fe0: 6d69 6e6e 6573 6f74 612c 2075 6e69 7465  minnesota, unite
00004ff0: 6420 7374 6174 6573 6f6d 616e 3234 3335  d statesoman2435
```

We could extract these value:
```
global computer services llc
globcom-oman.com
1998.0
computer software
51 - 200
burnsville, minnesota, united states
oman
24
35
```

Next, let's try to query using indexed column.

### Navigating indexes
The `idx_companies_country` starts at page 4, offset 12288.

```
00003000: 0200 0000 010f ee00 0000 073a 0fee 0000  ...........:....
#         || -> index interior
#           |---| -> zero freeblock
#                |---| -> 1 cell on page
#                     |---| -> 4079 start of cell content area
#                          || -> zero fragemented freeblock
#                             |--------| -> 7 right most pointer
#                                       |---| -> cell offset 4078


00003fe0: 0000 0000 0000 0000 0000 0000 0000 0000  ................
#                                            |---
00003ff0: 0739 0d03 1b03 6d79 616e 6d61 7214 56eb  .9....myanmar.V.
#         ---| -> 1849 left child pointer
#              || -> varint 13, payload length
#                |------------------------------| -> payload
#                ||  -> varint 3, header length
#                   || -> varint 27, string length = 7
#                     || -> varint 3, int
#                        |---------------| -> "myanmar"                        
#                                         |-----| -> row id 1,332,971, remaining bytes
00004000: 0d00 0000 2200 9500 0f7a 0f34 0ea8 0e38  ...."....z.4...8
00004010: 0dc9 0d62 0d07 0c80 0c14 0bc2 0b2a 0ab9  ...b.........*..
00004020: 0a52 09cd 0957 08f4 087e 0808 076e 072b  .R...W...~...n.+
00004030: 06c3 064a 05eb 0566 04dd 0476 0408 0389  ...J...f...v....
00004040: 0300 0291 0223 01c4 011f 0095 0000 0000  .....#..........
```
