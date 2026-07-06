use std::collections::{ HashMap };
use std::rc::Rc;
use std::u64;

use crate::parser::select::ValueExpr;
use crate::parser::{ Ast, ast_from_str };
use crate::exceptions::CustomErr;
use crate::pager::Pager;

#[derive(Debug)]
pub struct BTree {
    pub tables: HashMap<String, Table>,
    pub pager: Pager,
}

impl BTree {
    pub fn new(pager: Pager) -> Self {
        Self { tables: HashMap::new(), pager }
    }

    pub fn get_table(&self, table_name: &str) -> Option<&Table> {
        self.tables.get(table_name) 
    }

    pub fn parse_page(&mut self, page_no: &u64) -> Result<Page, CustomErr> {
        let buf = self.pager.read_page(page_no)?;
        let mut builder = PageBuilder::new(*page_no);

        // First page, skip first 100 bytes
        let mut offset: usize = if *page_no == 1 { 100 } else { 0 };
        
        // Parse the first page
        
        let page_type = PageType::try_from(get_offset(&buf, offset, 1)[0])?;
        offset += 1;

        // Start of freeblock
        let freeblock_offset = u16::from_be_bytes(get_offset(&buf, offset, 2).try_into()?) as u64;
        offset += 2;

        // Number of cell on page
        let cell_count = u16::from_be_bytes(get_offset(&buf, offset, 2).try_into()?) as u64;
        offset += 2;

        // Start of cell content area
        let cell_area_offset = u16::from_be_bytes(get_offset(&buf, offset, 2).try_into()?) as u64;
        offset += 2;

        // Number of fragmented free bytes
        let fragmented_freebyte_count = u8::from_be_bytes(get_offset(&buf, offset, 1).try_into()?) as u64;
        offset += 1;

        // Righ pointer, if any
        match &page_type {
            PageType::TblInterior | PageType::IdxInterior => {
                let right_ptr = u32::from_be_bytes(get_offset(&buf, 8, 4).try_into()?) as u64;
                offset += 4;
                builder.with_right_ptr(right_ptr);
            },
            _ => {}
        };

        // Cells offset, in increasing key order
        // physical position of cell does not matter
        for _ in 0..cell_count {
            let cell_offset = u16::from_be_bytes(get_offset(&buf, offset, 2).try_into()?) as u64;
            let cell = Cell::new(&page_type, &buf[cell_offset as usize..], &mut self.pager)?;
            offset += 2;
            builder.with_cell(cell_offset, cell);
        };
        
        builder.with_page_type(page_type)
            .with_freeblock_offset(freeblock_offset)
            .with_cell_count(cell_count)
            .with_cell_area_offset(cell_area_offset)
            .with_fragmented_freebyte_count(fragmented_freebyte_count);

        let page = builder.build()?;
        Ok(page)
    }

    pub fn parse_meta(&mut self) -> Result<(), CustomErr> {
        let page_01 = self.parse_page(&1)?;

        let mut builders_tbl: HashMap<String, TableBuilder> = HashMap::new();
        let mut builders_idx: Vec<IndexBuilder> = Vec::new(); 

        let cells = page_01.cells();
        for (offset, v) in cells.iter() {
            let Some(payload) = v.payload() else { continue };

            // sqlite_schema having columns in the following order
            // type, name, tbl_name, rootpage, sql
            // Let's building metadata

            let msg_col_index = "Invalid column index";
            let msg_type = "Wrong column type";

            // Type
            let type_col = payload.column(0)
                .ok_or(CustomErr::ValidateAST(msg_col_index.to_string()))?;
            let type_value = type_col.as_str()
                .ok_or(CustomErr::ValidateAST(msg_type.to_string()))?;
            // Page ptr

            let ptr_col = payload.column(3)
                .ok_or(CustomErr::ValidateAST(msg_col_index.to_string()))?;
            let ptr_value = ptr_col.as_int()
                .ok_or(CustomErr::ValidateAST(msg_type.to_string()))?;

            // Sql
            let sql_col = payload.column(4)
                .ok_or(CustomErr::ValidateAST(msg_col_index.to_string()))?;
            let sql_value = sql_col.as_str().ok_or(CustomErr::ValidateAST(msg_type.to_string()))?;

            let Some(ast) = ast_from_str(&sql_value)? else { continue };
            match type_value {
                "table" => {
                    let mut builder = TableBuilder::new();
                    let Ast::CreateTable(stmt) = ast else {
                        return Err(CustomErr::ValidateAST("Wrong stmt type".to_string()))
                    };
                    
                    let _ = builder.with_name(stmt.name())?;
                    let _ = builder.with_page_ptr(ptr_value as u64)?; 
                    let _ = builder.with_offset(*offset)?;

                    let columns = stmt.columns();
                    for (i, c) in columns.iter().enumerate() {
                        let _ = builder.with_column(c.name(), i as u64)?;
                    };

                    builders_tbl.insert(stmt.name().to_string(), builder);
                },
                "index" => {
                    let mut builder = IndexBuilder::new();
                    let Ast::CreateIndex(stmt) = ast else {
                        return Err(CustomErr::ValidateAST("Wrong stmt type".to_string()))
                    };

                    builder.with_name(stmt.name());
                    builder.with_table(stmt.table());
                    builder.with_column(stmt.column());
                    builder.with_page_ptr(ptr_value as u64);
                    builders_idx.push(builder);
                },
                _ => continue
            };

        };

        // Resolving index to table
        for b_idx in builders_idx.drain(..){
            let (index, table, column) = b_idx.build()?;
            let Some(b_tbl) = builders_tbl.get_mut(&table) else {
                continue
            };
            let _a = b_tbl.with_index(column.as_ref(), index);
        };

        for (k, v) in builders_tbl.drain() {
            let table = v.build()?;
            self.tables.insert(k, table);
        };

        Ok(())
    }
}

#[derive(Debug, PartialEq)]
pub enum Cell {
    TblInterior { left_child_ptr: u64, row_id: u64 },
    IdxInterior {
        left_child_ptr: u64,
        payload_length: u64, payload: CellPayload,
    },
    TblLeaf {
        payload_length: u64,
        row_id: u64,
        payload: CellPayload,
    },
    IdxLeaf {
        payload_length: u64,
        payload: CellPayload,
    }
}

impl Cell {
    pub fn new(page_type: &PageType, buf: &[u8], pager: &mut Pager) -> Result<Self, CustomErr> {
        match page_type {
            PageType::TblInterior => {
                let left_child_ptr = u32::from_be_bytes(get_offset(&buf, 0, 4).try_into()?) as u64;
                let (row_id, _row_bytes) = parse_varint(&buf[4..]);
                Ok(Cell::TblInterior { left_child_ptr, row_id })
            },
            PageType::IdxInterior => {
                let mut offset: usize = 0;
                let left_child_ptr = u32::from_be_bytes(get_offset(&buf, 0, 4).try_into()?) as u64;
                offset += 4;
                let (payload_length, payload_bytes) = parse_varint(&buf[offset..]);
                offset += payload_bytes;

                // Handle overflow page
                let payload: CellPayload = if offset + payload_length as usize > buf.len() {
                    let payload_buf = Self::get_overflow_payload(pager, &buf[offset..], payload_length)?;
                    CellPayload::try_from(&payload_buf[..])?
                } else {
                    CellPayload::try_from(&buf[offset..])?
                };
                Ok(Self::IdxInterior { left_child_ptr, payload_length, payload })
            },
            PageType::TblLeaf => {
                let mut offset: usize = 0;
                let (payload_length, payload_bytes) = parse_varint(&buf);
                offset += payload_bytes;
                let (row_id, row_id_bytes) = parse_varint(&buf[offset..]);
                offset += row_id_bytes;

                // There could be overflow page
                let mut payload: CellPayload = if offset + payload_length as usize > buf.len() {
                    let payload_buf = Self::get_overflow_payload(pager, &buf[offset..], payload_length)?;
                    CellPayload::try_from(&payload_buf[..])?
                } else {
                    CellPayload::try_from(&buf[offset..])?
                };
                payload.add_row_id(row_id);
                Ok(Self::TblLeaf { payload_length, row_id, payload })
            },
            PageType::IdxLeaf => {
                let mut offset: usize = 0;
                let (payload_length, payload_bytes) = parse_varint(&buf);
                offset += payload_bytes;

                // Overflow page
                let payload: CellPayload = if offset + payload_length as usize > buf.len() {
                    let payload_buf = Self::get_overflow_payload(pager, &buf[offset..], payload_length)?;
                    CellPayload::try_from(&payload_buf[..])?
                } else {
                    CellPayload::try_from(&buf[offset..])?
                };
                Ok(Self::IdxLeaf { payload_length, payload })
            }
        }
    }

    fn get_overflow_payload(pager: &mut Pager, buf: &[u8], payload_length: u64) -> Result<Vec<u8>, CustomErr> {
        let overflow_ptr = u32::from_be_bytes(*buf.last_chunk::<4>()
            .ok_or(CustomErr::Execution("Corrupted page: overflow payload".to_string()))?) as u64;
        let mut payload_bytes: Vec<u8> = Vec::from(&buf[..buf.len()-4]);
        let mut next_pointer: u64;

        loop {
            let next_buf = pager.read_page(&overflow_ptr)?;
            next_pointer = i32::from_be_bytes(next_buf[..4].try_into()?) as u64;
            payload_bytes.extend_from_slice(&next_buf[4..]);
            if next_pointer == 0 { break };
        };
        let (payload_bytes, _) = payload_bytes.split_at(payload_length as usize);
        Ok(Vec::from(payload_bytes))
    }

    pub fn left_ptr(&self) -> Option<u64> {
        match self {
            Self::IdxInterior { left_child_ptr, .. } | Self::TblInterior { left_child_ptr, .. } => {
                Some(*left_child_ptr)
            },
            _ => None
        }
    }

    pub fn indexed_value(&self) -> Option<ValueExpr> {
        match self {
            Self::IdxInterior { payload, .. } | Self::IdxLeaf { payload, .. } => {
                let col_value = payload.column(0).expect("Must be indexed value");
                ValueExpr::try_from(col_value).ok()
            },
            _ => None
        }
    }

    pub fn row_id(&self) -> Option<u64> {
        match self {
            Self::TblInterior { row_id, .. } | Self::TblLeaf { row_id, .. } => {
                Some(*row_id)
            },
            Self::IdxInterior { payload, .. } | Self::IdxLeaf { payload, .. } => {
                let cell_value =  payload.column(1).expect("Must be row_id");
                let row_id = cell_value.as_int().expect("Must be row_id") as u64;
                Some(row_id)
            }
        }
    }

    pub fn unpack_index(&self) -> Result<(u64, ValueExpr, u64), CustomErr> {
        let left_ptr = self.left_ptr();
        let indexed_value = self.indexed_value();
        let row_id = self.row_id();
        if left_ptr.is_some() && indexed_value.is_some() && row_id.is_some() {
            Ok((
                left_ptr.ok_or(CustomErr::Execution("Must be left_ptr".to_string()))?,
                indexed_value.ok_or(CustomErr::Execution("Must be indexed value".to_string()))?,
                row_id.ok_or(CustomErr::Execution("Must be row id".to_string()))?
            ))
        } else {
            Err(CustomErr::Execution("Corruped index".to_string())) 
        }
    }

    pub fn payload(&self) -> Option<&CellPayload> {
        match self {
            Self::IdxInterior { payload, .. } |
                Self::IdxLeaf { payload, .. } | Self::TblLeaf { payload, .. } => {
                Some(payload)
            },
            _ => None
        }
    }
}

#[derive(Debug, PartialEq, Clone)]
pub struct CellPayload {
    row_id: Option<u64>,
    columns: Vec<CellColValue>,
}

impl CellPayload {
    pub fn column(&self, index: usize) -> Option<&CellColValue> {
        self.columns.get(index)
    }

    pub fn last(&self) -> Result<&CellColValue, CustomErr> {
        self.columns.last().ok_or(CustomErr::Execution("Payload has not last value".to_string()))
    }

    pub fn add_row_id(&mut self, row_id: u64) {
        self.row_id = Some(row_id)
    }

    pub fn row_id(&self) -> Option<&u64> {
        self.row_id.as_ref()
    }
}

impl TryFrom<&[u8]> for CellPayload {
    type Error = CustomErr;

    fn try_from(buf: &[u8]) -> Result<Self, Self::Error> {
        let mut offset: usize = 0;

        // 1 varint is header length
        let (header_length, header_bytes) = parse_varint(&buf);
        offset += header_bytes;
        let mut remaining_bytes: usize = header_length as usize - header_bytes;
        
        let mut column_types: Vec<CellColType> = Vec::new();
        while remaining_bytes > 0 {
            let (value, value_bytes) = parse_varint(get_offset(&buf, offset, remaining_bytes));
            column_types.push(CellColType::new(value)?);
            offset += value_bytes;
            remaining_bytes -= value_bytes;
        };

        // Onto payload body
        let mut column_values: Vec<CellColValue> = Vec::new();
        for t in column_types {
            let value = CellColValue::new(&t, &buf[offset..])?;
            column_values.push(value);
            offset += t.content_length().unwrap_or(0);
        };
        Ok(Self { row_id: None, columns: column_values })
    }
}

// Type of column within Cell
pub enum CellColType {
    Null,
    Int8(usize),
    Int16(usize),     // Big endian from here on
    Int24(usize),     // No i24 in Rust
    Int32(usize),
    Int48(usize),
    Int64(usize),
    Float64(usize),
    IntLiteral0,    // Schema format 4 and higher
    IntLiteral1,
    Reserved,
    Blob(usize),
    Text(usize)
}

impl CellColType {
    pub fn new(serial_type: u64) -> Result<Self, CustomErr> {
        let col_type = match serial_type {
            0 => Self::Null,
            1 => Self::Int8(1),
            2 => Self::Int16(2),
            3 => Self::Int24(3), 
            4 => Self::Int32(4),
            5 => Self::Int48(5),
            6 => Self::Int64(6),
            7 => Self::Float64(6),
            8 => Self::IntLiteral0,
            9 => Self::IntLiteral1,
            10 | 11 => Self::Reserved,
            x if x >= 12 && x % 2 == 0 => Self::Blob((x as usize - 12)/2),
            x if x >= 13 && x % 2 != 0 => Self::Text((x as usize - 13)/2),
            _ => return Err(CustomErr::ParsePage("Unsupported serial type".to_string()))
        };
        Ok(col_type)
    }

    fn content_length(&self) -> Option<usize> {
        match self {
            Self::Int8(x) | Self::Int16(x) | Self::Int24(x) |
                Self::Int32(x) | Self::Int48(x) | Self::Int64(x) |
                Self::Float64(x) | Self::Blob(x) | Self::Text(x) => Some(*x),
            _ => None
        }
    }
}

// Value of column within cell
#[derive(Debug, PartialEq, Clone)]
pub enum CellColValue {
    Null,
    Int8(i8),
    Int16(i16),     // Big endian from here on
    Int24(i32),     // No i24 in Rust
    Int32(i32),
    Int48(i64),
    Int64(i64),
    Float64(f64),
    Blob(Vec<u8>),
    Text(String)
}

impl CellColValue {
    pub fn new(column_type: &CellColType, buf: &[u8]) -> Result<Self, CustomErr> {
        let Some(content_length) = column_type.content_length() else {
            return Ok(Self::Null)
        };

        let bytes = &buf[..content_length];
        match column_type {
            CellColType::Null => Ok(Self::Null),
            CellColType::Int8(_) => {
                Ok(Self::Int8(i8::from_be_bytes(bytes.try_into()?)))
            },
            CellColType::Int16(_) => {
                Ok(Self::Int16(i16::from_be_bytes(bytes.try_into()?)))
            },
            CellColType::Int24(_) => {
                let mut padded = vec![0u8; 4];
                padded[1..].copy_from_slice(bytes);
                Ok(Self::Int24(i32::from_be_bytes(padded.as_slice().try_into()?)))
            },
            CellColType::Int32(_) => {
                Ok(Self::Int32(i32::from_be_bytes(bytes.try_into()?)))
            },
            CellColType::Int48(_) => {
                let mut padded = vec![0u8; 8];
                padded[2..].copy_from_slice(bytes);
                Ok(Self::Int48(i64::from_be_bytes(padded.as_slice().try_into()?)))
            },
            CellColType::Int64(_) => {
                Ok(Self::Int64(i64::from_be_bytes(bytes.try_into()?)))
            },
            CellColType::Float64(_) => {
                Ok(Self::Float64(f64::from_be_bytes(bytes.try_into()?)))
            },
            CellColType::Blob(_) => {
                Ok(Self::Blob(Vec::from(bytes)))
            },
            CellColType::Text(_) => {
                Ok(Self::Text(String::from_utf8(Vec::from(bytes))?))
            },
            _ => Err(CustomErr::ParsePage("Invalid value".to_string())) 
        }
    }

    pub fn as_int(&self) -> Option<i64> {
        match self {
            Self::Int8(x) => Some(*x as i64),
            Self::Int16(x) => Some(*x as i64),
            Self::Int24(x) => Some(*x as i64),
            Self::Int32(x) => Some(*x as i64),
            Self::Int48(x) => Some(*x as i64),
            Self::Int64(x) => Some(*x as i64),
            _ => None
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::Text(s) => Some(s.as_ref()),
            _ => None
        }
    }

    pub fn as_bytes(&self) -> Option<&[u8]> {
        match self {
            Self::Blob(v) => Some(&v),
            _ => None
        }
    }

    pub fn to_string(&self) -> String {
        match self {
            Self::Int8(x) => x.to_string(),
            Self::Int16(x) => x.to_string(),
            Self::Int24(x) => x.to_string(),
            Self::Int32(x) => x.to_string(),
            Self::Int48(x) => x.to_string(),
            Self::Int64(x) => x.to_string(),
            Self::Float64(f) => f.to_string(),
            Self::Text(s) => s.to_string(),
            Self::Blob(_) => "".to_string(),
            Self::Null => "".to_string(),
        }
    }
}

// Table, created from AST
#[derive(Debug)]
pub struct Table {
    name: String,
    offset: u64,                    // offset bytes on page 1
    page_ptr: u64,
    columns_ord: Vec<Rc<str>>,
    columns: HashMap<Rc<str>, u64>, // column - order 
    indexes: HashMap<Rc<str>, Index>
}

impl Table{
    pub fn new(
        name: String,
        offset: u64,
        page_ptr: u64,
        columns_ord: Vec<Rc<str>>,
        columns: HashMap<Rc<str>, u64>,
        indexes: HashMap<Rc<str>, Index>
    ) -> Self {
        Self { name, offset, page_ptr, columns_ord, columns, indexes }
    }

    get_attr_str!(name);
    get_attr_u64!(offset);
    get_attr_u64!(page_ptr);

    pub fn index(&self, column_name: &str) -> Option<&Index> {
        self.indexes.get(&Rc::from(column_name))
    }

    pub fn column_index(&self, column_name: &str) -> Option<&u64> {
        self.columns.get(&Rc::from(column_name))
    }

    pub fn columns_ord(&self) -> &[Rc<str>] {
        self.columns_ord.as_ref()
    }
}



// Table Builder
pub struct TableBuilder {
    name: Option<String>,
    offset: Option<u64>,
    page_ptr: Option<u64>,
    columns_ord: Option<Vec<Rc<str>>>,
    columns: Option<HashMap<Rc<str>, u64>>,
    indexes: Option<HashMap<Rc<str>, Index>>,
}

impl TableBuilder {
    pub fn new() -> Self {
        Self {
            name: None,
            offset: None,
            page_ptr: None,
            columns_ord: None,
            columns: None,
            indexes: None
        }
    }

    pub fn with_name(&mut self, name: &str) -> Result<&mut Self, CustomErr> {
        self.name = Some(name.to_string());
        Ok(self)
    }

    pub fn with_offset(&mut self, offset: u64) -> Result<&mut Self, CustomErr> {
        self.offset = Some(offset);
        Ok(self)
    }

    pub fn with_page_ptr(&mut self, page_ptr: u64) -> Result<&mut Self, CustomErr> {
        self.page_ptr = Some(page_ptr);
        Ok(self)
    }

    pub fn with_column(&mut self, column_name: &str, ord: u64) -> Result<&mut Self, CustomErr> {
        let column_rc: Rc<str> = Rc::from(column_name);
        self.columns_ord.get_or_insert_default().push(column_rc.clone());
        let _ = self.columns.get_or_insert_default().insert(column_rc, ord);
        Ok(self)
    }

    pub fn with_index(&mut self, column_name: &str, index: Index) -> Result<&mut Self, CustomErr> {
        let column_rc: Rc<str> = Rc::from(column_name);
        let Some(set) = &mut self.columns else {
            return Err(CustomErr::ValidateAST(format!("Column named {} does not exist", column_name))) 
        };
        let Some((k, _)) = set.get_key_value(&column_rc) else {
            return Err(CustomErr::ValidateAST(format!("Column named {} does not exist", column_name)))};
        let _ = self.indexes.get_or_insert_default().insert(k.clone(), index);
        Ok(self)
    }

    pub fn build(self) -> Result<Table, CustomErr> {
        Ok(Table::new(
            self.name.ok_or(CustomErr::ValidateAST("Table name not provided".to_string()))?,
            self.offset.ok_or(CustomErr::ValidateAST("Cell offset not provided".to_string()))?,
            self.page_ptr.ok_or(CustomErr::ValidateAST("Page pointer not provided".to_string()))?,
            self.columns_ord.ok_or(CustomErr::ValidateAST("Columns not provided".to_string()))?,
            self.columns.ok_or(CustomErr::ValidateAST("Columns not provided".to_string()))?, 
            self.indexes.unwrap_or_default()
            ))
    }
}

#[derive(Debug)]
pub struct Index {
    name: String,
    page_ptr: u64,
}

impl Index {
    pub fn new(name: String, page_ptr: u64) -> Self {
        Self { name, page_ptr }
    }

    get_attr_str!(name);
    get_attr_u64!(page_ptr);
}

pub struct IndexBuilder {
    name: Option<String>,
    table: Option<String>,
    column: Option<String>,
    page_ptr: Option<u64>,
}

impl IndexBuilder {
    pub fn new() -> Self {
        Self { name: None, table: None, column: None, page_ptr: None }
    }

    build_attr_String!(name);
    build_attr_String!(table);
    build_attr_String!(column);
    build_attr_u64!(page_ptr);

    pub fn build(self) -> Result<(Index, String, String), CustomErr> {
        // Return Index, table name, column name
        let index = Index::new(
            self.name
                .ok_or(CustomErr::ValidateAST("Index name not provided".to_string()))?,
            self.page_ptr
                .ok_or(CustomErr::ValidateAST("Page pointer not provided".to_string()))?
            );
        let Some(table) = self.table else {
            return Err(CustomErr::ValidateAST("Indexed table not provided".to_string()))
        };
        let Some(column) = self.column else {
            return Err(CustomErr::ValidateAST("Indexed column not provided".to_string()))
        };
        Ok((index, table, column))
    }
}

#[derive(Debug, PartialEq)]
pub struct Page {
    page_ptr: u64,
    page_type: PageType,
    freeblock_offset: u64,
    cell_count: u64,
    cell_area_offset: u64,
    fragmented_freebyte_count: u64,
    right_ptr: Option<u64>,
    cells_order: Vec<u64>,
    cells: HashMap<u64, Cell>,
}

impl Eq for Page {}

impl Page {
    get_attr_u64!(page_ptr);
    get_attr_u64!(freeblock_offset);
    get_attr_u64!(cell_count);
    get_attr_u64!(cell_area_offset);
    get_attr_u64!(fragmented_freebyte_count);

    pub fn page_type(&self) -> &PageType {
        &self.page_type
    }

    pub fn right_ptr(&self) -> Option<&u64> {
        self.right_ptr.as_ref()
    }

    pub fn cells_order(&self) -> &[u64] {
        &self.cells_order
    }

    pub fn cells(&self) -> &HashMap<u64, Cell> {
        &self.cells
    }

    pub fn cell(&self, offset: &u64) -> Result<&Cell, CustomErr> {
        self.cells.get(offset).ok_or(CustomErr::Execution("Invalid cell offset".to_string()))
    }
}

#[derive(Debug, PartialEq)]
pub enum PageType {
    TblInterior,
    IdxInterior,
    TblLeaf,
    IdxLeaf,
}

impl Eq for PageType {}

impl TryFrom<u8> for PageType {
    type Error = CustomErr;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0x02 => Ok(Self::IdxInterior),
            0x05 => Ok(Self::TblInterior),
            0x0a => Ok(Self::IdxLeaf),
            0x0d => Ok(Self::TblLeaf),
            _ => Err(CustomErr::ParsePage("Invalid page type".to_string()))
        } 
    }
}



pub struct PageBuilder {
    // All offset values is from beginning of page
    // so they never exceed 65536
    page_ptr: u64,
    page_type: Option<PageType>,
    freeblock_offset: Option<u64>,
    cell_count: Option<u64>,
    cell_area_offset: Option<u64>,
    fragmented_freebyte_count: Option<u64>,
    right_ptr: Option<u64>,             // only in interior page
    cells_order: Vec<u64>,
    cells: HashMap<u64, Cell>,
}

impl PageBuilder {
    pub fn new(page_ptr: u64) -> Self {
        Self {
            page_ptr,
            page_type: None,
            freeblock_offset: None,
            cell_count: None,
            cell_area_offset: None,
            fragmented_freebyte_count: None,
            right_ptr: None,
            cells_order: Vec::new(),
            cells: HashMap::new(),
        }
    }

    pub fn with_page_type(&mut self, page_type: PageType) -> &mut Self {
        self.page_type = Some(page_type);
        self
    }

    build_attr_u64!(freeblock_offset);
    build_attr_u64!(cell_count);
    build_attr_u64!(cell_area_offset);
    build_attr_u64!(fragmented_freebyte_count);
    build_attr_u64!(right_ptr);

    pub fn with_cell(&mut self, offset: u64, cell: Cell) -> &mut Self {
        self.cells_order.push(offset);
        self.cells.insert(offset, cell);
        self
    }

    pub fn build(self) -> Result<Page, CustomErr> {
        Ok(Page {
            page_ptr: self.page_ptr,
            page_type: self.page_type
                .ok_or(CustomErr::ParsePage("Page type".to_string()))?,
            freeblock_offset: self.freeblock_offset
                .ok_or(CustomErr::ParsePage("Free block".to_string()))?,
            cell_count: self.cell_count
                .ok_or(CustomErr::ParsePage("Cell count".to_string()))?,
            cell_area_offset: self.cell_area_offset
                .ok_or(CustomErr::ParsePage("Cell area offset".to_string()))?,
            fragmented_freebyte_count: self.fragmented_freebyte_count
                .ok_or(CustomErr::ParsePage("Fragmented freebyte count".to_string()))?,
            right_ptr: self.right_ptr,
            cells_order: self.cells_order,
            cells: self.cells
        })
    }
}

//------------------------------------------------
struct CellBuilder {
    left_child_ptr: Option<u64>,
    payload_length: Option<u64>,
    row_id: Option<u64>,
    payload: Option<CellPayload>,
    overflow_ptr: Option<u64>
}

impl CellBuilder {
    pub fn new() -> Self {
        Self {
            left_child_ptr: None,
            payload_length: None, 
            row_id: None, 
            payload: None,
            overflow_ptr: None
        }
    }

    pub fn with_payload(&mut self, payload: CellPayload) -> &mut Self {
        self.payload = Some(payload);
        self
    }

    build_attr_u64!(left_child_ptr);
    build_attr_u64!(payload_length);
    build_attr_u64!(row_id);
    build_attr_u64!(overflow_ptr);
}

fn get_offset(buf: &[u8], offset: usize, length: usize) -> &[u8] {
    &buf[offset..offset+length]
}

fn parse_varint(buf: &[u8]) -> (u64, usize) {
    // convert a slice of bytes to u64, big endian
    let mut output = 0u64;

    // Up to 9 bytes could be use
    for i in 0..9 {
        let b = buf[i];
        if i == 8 {
            output = (output << 8) | b as u64;
            return (output, 9)
        };
        
        output = (output << 7) | (b & 0x7f) as u64;
        // Check bit at 0
        if b & 0x80 == 0 {
            return (output, i + 1)
        };
    };

    (output, 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_page_01 () {
        let pager = Pager::new("companies.db");
        let mut btree = BTree::new(pager);
        let page_01 = btree.parse_page(&1);
        println!("Page 01: {:?}", &page_01);
        assert!(page_01.is_ok());

        // Page 2, table companies
        let page_2 = btree.parse_page(&2);
        println!("Page 2: {:?}", &page_2);
        assert!(page_2.is_ok());

        // Parse metadata
        let result = btree.parse_meta();
        println!("Parse meta result {:?}", &result);
        println!("Btree {:?}", &btree);

        // Index on country
        let page_4 = btree.parse_page(&4);
        println!("Page 4: {:?}", &page_4);
    }
}
