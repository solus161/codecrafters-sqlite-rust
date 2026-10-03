use std::collections::hash_map::Entry;
use std::collections::{HashMap, VecDeque};

use crate::btree::{BTree, Cell, CellPayload, Page, PageType};
use crate::exceptions::CustomErr;
use crate::parser::select::{ColumnExpr, CompSpecOp, IndexCondition, SelectStmt};
use crate::parser::{Ast, Eval, ast_from_str};

pub struct QueryOutput {
    rows: Vec<String>,
}

impl QueryOutput {
    pub fn new() -> Self {
        Self { rows: Vec::new() }
    }
}

pub fn process(btree: &mut BTree, s: &str) -> Result<String, CustomErr> {
    // TODO: String is a simple output, thing could be more complicated
    let ast = ast_from_str(s)?.ok_or(CustomErr::Execution("Blank AST".to_string()))?;

    match ast {
        Ast::Select(mut stmt) => {
            // Execution order
            // 1. Locate target table: rootpage
            // 2. Full scan vs index
            // 3. Walk the tree, apply filter, get the row/Cell

            // From clause first
            let table_name = stmt
                .from_clause
                .table_name()
                .ok_or(CustomErr::ValidateAST("Table name not found".to_string()))?;

            let table = btree
                .get_table(table_name)
                .ok_or(CustomErr::Execution("Table not found".to_string()))?;

            // Match column name in where with index in payload
            stmt.resolve(table)?;

            // Where clause
            // WhereExpr in WhereClause need to be compiled into
            // somekind of fn that eval each Cell payload to True/False
            let index_cond = stmt.resolve_index();
            let have_index = match &index_cond {
                Some(cond) => table.index(cond.column()).is_some(),
                None => false,
            };

            // We just need one index to walk one tree
            let payloads = if !have_index {
                let start_ptr = table.page_ptr();
                full_scan(btree, &stmt, start_ptr)?
            } else {
                let index_filter = index_cond.expect("Index could be inplace");
                let index = table
                    .index(index_filter.column())
                    .expect("Index should be there");
                let idx_start_ptr = index.page_ptr();
                let tbl_start_ptr = table.page_ptr();
                index_scan(btree, &index_filter, &stmt, idx_start_ptr, tbl_start_ptr)?
            };

            // Got the rows, how choose what to return
            let mut output = extract_columns(btree, payloads, &stmt);
            output.sort();
            Ok(output.join("\n"))
        }
        _ => Err(CustomErr::ValidateAST("AST not supported".to_string())),
    }
}

fn full_scan(
    btree: &mut BTree,
    filter: &SelectStmt,
    start_ptr: u64,
) -> Result<Vec<CellPayload>, CustomErr> {
    let mut page_queue: VecDeque<u64> = VecDeque::new();
    page_queue.push_back(start_ptr);
    let mut payloads = Vec::new();

    while !page_queue.is_empty() {
        // Let walk the tree
        let Some(page_ptr) = page_queue.pop_front() else {
            break;
        };
        let page = btree.parse_page(&page_ptr)?;

        // Ptr to the right-most subtree, which no cell points at.
        // Only once: pushing it twice walks that subtree twice and duplicates its rows.
        if let Some(right_ptr) = page.right_ptr() {
            page_queue.push_back(*right_ptr);
        };

        // Cells, include page type here
        let cells = page.cells();
        for cell in cells.values() {
            match cell {
                Cell::TblInterior { left_child_ptr, .. } => {
                    page_queue.push_back(*left_child_ptr);
                }
                Cell::TblLeaf { payload, .. } => {
                    if filter.eval_where(payload)? {
                        payloads.push(payload.clone());
                    };
                }
                _ => {
                    return Err(CustomErr::Execution(
                        "Should not be an index page".to_string(),
                    ));
                }
            }
        }
    }

    Ok(payloads)
}

fn index_scan(
    btree: &mut BTree,
    idx_filter: &IndexCondition,
    tbl_filter: &SelectStmt,
    idx_start_ptr: u64,
    tbl_start_ptr: u64,
) -> Result<Vec<CellPayload>, CustomErr> {
    let mut idx_queue: VecDeque<u64> = VecDeque::new(); // left and righ ptr
    let mut row_id_queue: Vec<u64> = Vec::new();

    idx_queue.push_back(idx_start_ptr);

    // Walk the index tree first
    while !idx_queue.is_empty() {
        let Some(page_ptr) = idx_queue.pop_front() else {
            break;
        };
        let page = btree.parse_page(&page_ptr)?;
        let page_type = page.page_type();
        let right_ptr = page.right_ptr();

        let target = idx_filter.target();
        // Cells, include page type here
        let cells_offset = page.cells_order();

        match page_type {
            PageType::IdxInterior => {
                // Evaluate on all cells of a node, to get next node
                match idx_filter.op() {
                    CompSpecOp::Eq => {
                        // Collect the whole run of entries whose indexed value == target.
                        // The run may span several separators and their left subtrees,
                        // since the key order is the tuple (indexed_value, row_id): a
                        // large run promotes many equal separators into this interior page.
                        let mut past_run = false;
                        for offset in cells_offset.iter() {
                            let cell = page.cell(offset)?;
                            let (left_ptr, idx_value, row_id) = cell.unpack_index()?;
                            if idx_value < *target {
                                // This cell and its left subtree are entirely below the run
                                continue;
                            } else if idx_value == *target {
                                // The separator itself is a match; its left subtree may hold
                                // earlier duplicates (smaller row_id). Run may extend right,
                                // so keep scanning.
                                row_id_queue.push(row_id);
                                idx_queue.push_back(left_ptr);
                            } else {
                                // First cell past target: its left subtree holds the tail of
                                // the run, and nothing further right can match. Stop here and
                                // do NOT descend the right pointer.
                                idx_queue.push_back(left_ptr);
                                past_run = true;
                                break;
                            };
                        }
                        // If no separator exceeded target, the run may spill into the
                        // right-most subtree.
                        if !past_run && let Some(ptr) = right_ptr {
                            idx_queue.push_back(*ptr);
                        };
                    }
                    CompSpecOp::Ne => {
                        // != is served by a full table scan, so the planner never picks
                        // an index for it (IndexCondition::from_where_cond returns None)
                        // and this walk should never be reached with it.
                        return Err(CustomErr::Execution(
                            "Ne cannot be served by an index scan".to_string(),
                        ));
                    }
                    CompSpecOp::Gt | CompSpecOp::Ge => {
                        // Mirror of Lt/Le: entries above the bound form a suffix of the
                        // key order. A separator below the bound has its whole left
                        // subtree below it too, so both are skipped outright.
                        let inclusive = matches!(idx_filter.op(), CompSpecOp::Ge);
                        for offset in cells_offset.iter() {
                            let cell = page.cell(offset)?;
                            let (left_ptr, idx_value, row_id) = cell.unpack_index()?;
                            let in_bound = if inclusive {
                                idx_value >= *target
                            } else {
                                idx_value > *target
                            };

                            if in_bound {
                                // The separator itself matches. Its left subtree spans the
                                // gap below this key and can still hold matches -- when the
                                // values are equal, the earlier duplicates of the run.
                                row_id_queue.push(row_id);
                                idx_queue.push_back(left_ptr);
                            };
                        }

                        // The right-most subtree is above every separator on this page,
                        // so it can always hold matches.
                        if let Some(ptr) = right_ptr {
                            idx_queue.push_back(*ptr);
                        };
                    }
                    CompSpecOp::Lt | CompSpecOp::Le => {
                        // Entries below the bound form a prefix of the key order, since
                        // the key is the tuple (indexed_value, row_id) and so orders on
                        // indexed_value first. Walk the separators in ascending order and
                        // take everything up to the first one past the bound.
                        let inclusive = matches!(idx_filter.op(), CompSpecOp::Le);
                        let mut past_bound = false;
                        for offset in cells_offset.iter() {
                            let cell = page.cell(offset)?;
                            let (left_ptr, idx_value, row_id) = cell.unpack_index()?;
                            let in_bound = if inclusive {
                                idx_value <= *target
                            } else {
                                idx_value < *target
                            };

                            if in_bound {
                                // The separator itself matches, and its left subtree is
                                // entirely below it, so that whole subtree matches too.
                                row_id_queue.push(row_id);
                                idx_queue.push_back(left_ptr);
                            } else {
                                // First separator past the bound: its left subtree spans
                                // the gap below this key and may hold the tail of the
                                // prefix. Nothing further right can match, so stop here
                                // and do NOT descend the right pointer.
                                idx_queue.push_back(left_ptr);
                                past_bound = true;
                                break;
                            };
                        }
                        // Every separator was in bound, so the prefix may spill into the
                        // right-most subtree.
                        if !past_bound && let Some(ptr) = right_ptr {
                            idx_queue.push_back(*ptr);
                        };
                    }
                }
            }
            PageType::IdxLeaf => {
                // Got the the leaf now, just extract row id
                // Filter applied to all cells
                for offset in cells_offset {
                    let cell = page.cell(offset)?;
                    let payload = cell.payload().expect("Must be payload");
                    let eval_output = idx_filter.eval(payload)?;
                    if eval_output.is_true() {
                        row_id_queue.push(cell.row_id().expect("Must be row id"));
                    };
                }
            }
            _ => { /* not applicable */ }
        };
    }

    // Extract table rows
    // First filter the row id list
    row_id_queue.sort();
    let payloads = extract_rows(btree, &row_id_queue, tbl_filter, tbl_start_ptr)?;

    Ok(payloads)
}

pub fn extract_rows(
    btree: &mut BTree,
    row_ids: &[u64],
    tbl_filter: &SelectStmt,
    tbl_start_ptr: u64,
) -> Result<Vec<CellPayload>, CustomErr> {
    let mut page_cache: HashMap<u64, Page> = HashMap::new();
    let mut page_queue: VecDeque<u64> = VecDeque::new();

    let mut output: Vec<CellPayload> = Vec::new();
    for row_id in row_ids {
        page_queue.push_back(tbl_start_ptr);

        'walk: while !page_queue.is_empty() {
            let page_ptr = page_queue
                .pop_front()
                .expect("Page queue must not be empty");
            let page =
                if let std::collections::hash_map::Entry::Vacant(e) = page_cache.entry(page_ptr) {
                    let page = btree.parse_page(&page_ptr)?;
                    e.insert(page);
                    page_cache.get(&page_ptr).expect("Page must be there")
                } else {
                    page_cache.get(&page_ptr).expect("Page must be there")
                };

            // Walk the tbl tree
            match page.page_type() {
                PageType::TblInterior => {
                    // println!("Page cell offset {:?}", page.cells_order());
                    for offset in page.cells_order() {
                        let cell = page.cell(offset)?;
                        let tree_row_id = cell.row_id().expect("Corrupted page");
                        if tree_row_id >= *row_id {
                            // println!("Got upper bound row id {}, given row id {}", &tree_row_id, row_id);
                            let left_ptr = cell.left_ptr().expect("Corrupted page");
                            // println!("Push left ptr to queue {}", &left_ptr);
                            page_queue.push_back(left_ptr);
                            continue 'walk;
                        };
                    }
                    if let Some(right_ptr) = page.right_ptr() {
                        page_queue.push_back(*right_ptr);
                    }
                }
                PageType::TblLeaf => {
                    // println!("Got to leaf page");
                    for cell in page.cells().values() {
                        let payload = cell.payload().expect("Corrupted page");
                        if cell.row_id() == Some(*row_id) && tbl_filter.eval_where(payload)? {
                            output.push(payload.clone());
                        }
                    }
                }
                _ => return Err(CustomErr::Execution("Corrupted page".to_string())),
            };
        }
    }

    Ok(output)
}

fn extract_columns(
    btree: &BTree,
    mut payloads: Vec<CellPayload>,
    stmt: &SelectStmt,
) -> Vec<String> {
    // Only support column at the moment, no Count or Group By
    let mut columns_index: Vec<i64> = Vec::new();

    let table_name = stmt
        .from_clause
        .table_name()
        .expect("Only support literal column");
    let table = btree.get_table(table_name).expect("Table not exists");

    let select_clause = stmt.select_clause.columns();
    let mut output = Vec::new();
    let mut early_return = false;
    select_clause.iter().for_each(|c| {
        match c {
            ColumnExpr::Name { name, .. } => {
                if name.as_str() == "id" {
                    // id is a default column
                    columns_index.push(-1);
                } else {
                    // let name_rc: Rc<&str> = Rc::from(name.as_str());
                    let idx = *table
                        .column_index(name.as_str())
                        .expect("Column must exists");
                    columns_index.push(idx as i64);
                }
            }
            ColumnExpr::All => {
                columns_index.push(-1);
                table.columns_ord().iter().for_each(|c| {
                    let idx = *table.column_index(c).expect("Columns must exists");
                    columns_index.push(idx as i64);
                });
            }
            ColumnExpr::Count(_) => {
                // Support a single count at the moment, no group by
                let count = payloads.len().to_string();
                output.push(count);
                early_return = true;
            }
        }
    });

    if early_return {
        return output;
    };

    payloads.drain(..).for_each(|p| {
        let mut row: Vec<String> = Vec::new();
        for i in columns_index.iter() {
            if *i == -1 {
                row.push(
                    p.row_id()
                        .expect("Table leaf cell must have row id")
                        .to_string(),
                );
            } else {
                let col_value = p.column(*i as usize).expect("Must be CellColValue");
                row.push(col_value.to_string());
            };
        }
        output.push(row.join("|"));
    });
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Pager;

    #[test]
    fn test_fullscan_and() {
        let s = "select * from companies \
                where locality = 'burnsville, minnesota, united states' and \
                year_founded = '1998.0'";
        let pager = Pager::new("companies.db");
        let mut btree = BTree::new(pager);
        let _ = btree.parse_meta();
        let output = process(&mut btree, s);
        println!("Fullscan payloads: {:?}", &output);
        assert!(output.is_ok());
    }

    #[test]
    fn test_fullscan_or() {
        let s = "select * from companies \
                where locality = 'burnsville, minnesota, united states' or \
                year_founded = '1998.0'";
        let pager = Pager::new("companies.db");
        let mut btree = BTree::new(pager);
        let _ = btree.parse_meta();
        let output = process(&mut btree, s);
        println!("Full scan payloads: {:?}", &output);
        assert!(output.is_ok());
    }

    #[test]
    fn test_index_scan() {
        // let s = "select * from companies where country = 'american samoa';";
        let s = "select * from companies where country = 'oman';";
        // let s = "select * from companies where country = 'anantigua and barbuda';";
        let pager = Pager::new("companies.db");
        let mut btree = BTree::new(pager);
        let _ = btree.parse_meta();
        let output = process(&mut btree, s);
        println!("Index scan payloads: {:?}", &output);
        assert!(output.is_ok());
    }

    #[test]
    fn test_index_scan_01() {
        // let s = "select * from companies where country = 'american samoa';";
        let s = "select * from companies where country > 'yemen';";
        // let s = "select * from companies where country = 'anantigua and barbuda';";
        let pager = Pager::new("companies.db");
        let mut btree = BTree::new(pager);
        let _ = btree.parse_meta();
        let output = process(&mut btree, s);
        println!("Index scan GT payloads: {:?}", &output);
        assert!(output.is_ok());
    }

    // Every expected count below was taken from sqlite3 on the same file:
    //   sqlite3 companies.db "select count(*) from companies where <cond>"
    // `country` carries idx_companies_country, so the range ops go through
    // index_scan while != falls back to full_scan.
    fn count_where(cond: &str) -> u64 {
        let pager = Pager::new("companies.db");
        let mut btree = BTree::new(pager);
        let _ = btree.parse_meta();
        let s = format!("select count(*) from companies where {}", cond);
        let output = process(&mut btree, &s).expect("Query must succeed");
        output.trim().parse().expect("Count must be a number")
    }

    #[test]
    fn test_index_scan_lt() {
        // 'afghanistan' is the smallest indexed value, so nothing is below it
        assert_eq!(count_where("country < 'afghanistan'"), 0);

        // Bound lands exactly on a value: exclude its run, keep what precedes
        assert_eq!(count_where("country < 'albania'"), 376);

        // Bound falls between two values, so the run below it stays complete
        assert_eq!(count_where("country < 'albanib'"), 1151);
    }

    #[test]
    fn test_index_scan_le() {
        // Below every indexed value
        assert_eq!(count_where("country <= 'aaa'"), 0);

        // Bound on the smallest value: its whole run must come back
        assert_eq!(count_where("country <= 'afghanistan'"), 376);

        assert_eq!(count_where("country <= 'albania'"), 1151);
    }

    #[test]
    fn test_index_scan_gt() {
        // Above every indexed value
        assert_eq!(count_where("country > 'zzz'"), 0);

        assert_eq!(count_where("country > 'yemen'"), 528);

        // Bound between two values: the run above it stays complete
        assert_eq!(count_where("country > 'zambib'"), 48);
    }

    #[test]
    fn test_index_scan_ge() {
        // Bound on a value: its run is included, so this exceeds the > case
        assert_eq!(count_where("country >= 'yemen'"), 643);

        // A run whose separator sits last on an interior page used to be dropped
        assert_eq!(count_where("country >= 'zambia'"), 528);

        // Bound on the smallest value: every row matches
        assert_eq!(count_where("country >= 'afghanistan'"), 55991);
    }

    #[test]
    fn test_fullscan_ne() {
        // != cannot use the index, so this must come back via a full scan
        assert_eq!(count_where("country != 'oman'"), 54654);

        // Matches every row, so it also pins down that full_scan visits each
        // row exactly once -- the right-most subtree used to be walked twice
        assert_eq!(count_where("country != 'zzz'"), 55991);
    }
}
