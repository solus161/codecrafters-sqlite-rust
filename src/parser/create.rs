use paste;

use crate::exceptions::CustomErr;
use super::tokenizer::{Token, Tokens};
use super::Build;


// CREATE TABLE and CREATE INDEX having different structure
// so they deserve different ASTs
#[derive(Debug, PartialEq)]
pub struct CreateTableStmt {
    name: String,
    columns: ColumnClause,
}

impl CreateTableStmt {
    pub fn new(name: String, columns: ColumnClause) -> Self {
        Self { name, columns }
    }

    get_attr_str!(name);

    pub fn columns(&self) -> &[ColumnAttr] {
        self.columns.columns()
    }
}

impl Build for CreateTableStmt {
    fn build(tokens: &mut Tokens) -> Result<Option<Self>, CustomErr> {
        let mut name: Option<String> = None; 
        let mut target_tokens: Option<Vec<Token>> = Some(vec![Token::Create { start: 0 }]);

        loop {
            let Some(_) = tokens.peek(0) else { break };
            let Some(token) = tokens.current() else { break };
            if !token.is_token_type_matched(&target_tokens) {
                return Err(tokens.get_syntax_error())
            };

            match token {
                Token::Create { .. } => target_tokens = Some(vec![Token::Table { start: 0 }]),
                Token::Table { .. } => target_tokens = Some(vec![Token::Ident { start: 0, value: "" }]),
                Token::Ident { value, .. } => {
                    name = Some((*value).to_string());
                    target_tokens = Some(vec![Token::Lparen { start: 0 }])
                },
                Token::Lparen { .. } => {
                    let column_clause = ColumnClause::build(tokens)?
                        .expect("Never return None");
                    if column_clause.0.is_empty() {
                        return Err(tokens.get_syntax_error())
                    };

                    let Some(rparen) = tokens.current() else {
                        return Err(tokens.get_syntax_error())
                    };

                    if !matches!(rparen, Token::Rparen { .. }) {
                        return Err(tokens.get_syntax_error())
                    };
                    return Ok(Some(
                            Self::new(
                                name.ok_or(tokens.get_syntax_error())?,
                                column_clause)
                            ))
                },
                _ => return Err(tokens.get_syntax_error())
            }
        };
        Ok(None)
    }
}

// Column clause
#[derive(Debug, PartialEq)]
pub struct ColumnClause(Vec<ColumnAttr>);

impl Eq for ColumnClause {}

impl Build for ColumnClause {
    fn build(tokens: &mut Tokens) -> Result<Option<Self>, CustomErr> {
        let mut target_tokens: Option<Vec<Token>> = Some(vec![Token::Ident { start: 0, value: "" }]);
        let mut columns: Vec<ColumnAttr> = Vec::new();

        loop {
            let Some(current_token) = tokens.peek(0) else { 
                return Ok(Some(Self(columns))) 
            };

            if matches!(current_token, Token::Rparen { .. }) {
                return Ok(Some(Self(columns)))
            };

            let Some(token) = tokens.current() else { break };
            if !token.is_token_type_matched(&target_tokens) {
                return Err(tokens.get_syntax_error())
            };

            match token {
                Token::Ident { value, .. } => {
                    // Next must be type
                    // let Some(token_type) = tokens.current() else {
                    //     return Err(tokens.get_syntax_error())
                    // };
                    //
                    // let col_type = ColumnType::try_from(token_type)?;
                    //
                    // let col_attr = ColumnAttr::new(value.to_string(), col_type);
                    // columns.push(col_attr);
                    columns.push(ColumnAttr::new(value.to_string()));
                    
                    target_tokens = Some(vec![
                        Token::Text { start: 0 },
                        Token::Integer { start: 0 },
                        Token::Float { start: 0 },
                        Token::PrimaryKey { start: 0 },
                        Token::AutoIncrement { start: 0 },
                        Token::Comma { start: 0 }
                    ])
                },
                Token::Text { .. } | Token::Integer { .. } | Token::Float { .. } => {
                    let Some(col_top) = columns.last_mut() else {
                        return Err(tokens.get_syntax_error())
                    };
                    col_top.set_type(ColumnType::try_from(token)?);
                    target_tokens = Some(vec![
                        Token::PrimaryKey { start: 0 },
                        Token::AutoIncrement { start: 0 },
                        Token::Comma { start: 0 }
                    ])
                },
                Token::PrimaryKey { .. } => {
                    let Some(col_top) = columns.last_mut() else {
                        return Err(tokens.get_syntax_error())
                    };

                    col_top.set_pk(true);
                    target_tokens = Some(vec![
                        Token::AutoIncrement { start: 0 },
                        Token::Comma { start: 0 }
                    ])
                },
                Token::AutoIncrement { .. } => {
                    let Some(col_top) = columns.last_mut() else {
                        return Err(tokens.get_syntax_error())
                    };

                    col_top.set_default(DefaultExpr::AutoIncrement);
                    target_tokens = Some(vec![
                        Token::Comma { start: 0 }
                    ])
                },
                Token::Comma { .. } => {
                    target_tokens = Some(vec![
                        Token::Ident { start: 0, value: "" }
                    ])
                }
                _ => return Err(tokens.get_syntax_error())
            }
        };
        
        Ok(Some(Self(columns)))
    }
}

impl ColumnClause {
    pub fn columns(&self) -> &[ColumnAttr] {
        &self.0
    }
}

// Clause that describes a column
#[derive(Debug, PartialEq)]
pub struct ColumnAttr {
    name: String,
    column_type: Option<ColumnType>,
    pk: bool,
    default: Option<DefaultExpr>
}

impl Eq for ColumnAttr {}

impl ColumnAttr {
    pub fn new(name: String) -> Self {
        Self {
            name: name,
            column_type: None,
            pk: false,
            default: None
        }
    }

    get_attr_str!(name);

    pub fn column_type(&self) -> Option<&ColumnType> {
        self.column_type.as_ref()
    }

    pub fn set_type(&mut self, t: ColumnType) {
        self.column_type = Some(t)
    }

    pub fn pk(&self) -> bool {
        self.pk
    }

    pub fn default(&self) -> Option<&DefaultExpr> {
        self.default.as_ref()
    }

    pub fn set_pk(&mut self, pk: bool) {
        self.pk = pk
    }

    pub fn set_default(&mut self, default: DefaultExpr) {
        self.default = Some(default)
    }
}

#[derive(Debug, PartialEq)]
pub enum DefaultExpr {
    AutoIncrement,
}

impl Eq for DefaultExpr {}

#[derive(Debug, PartialEq)]
pub enum ColumnType {
    Text,
    Integer,
    Float,
}

impl Eq for ColumnType {}

impl TryFrom<&Token<'_>> for ColumnType {
    type Error = CustomErr;

    fn try_from(value: &Token) -> Result<Self, Self::Error> {
        let col_type = match value {
            Token::Text { .. } => Self::Text,
            Token::Integer { .. } => Self::Integer,
            Token::Float { .. } => Self::Float,
            _ => return Err(CustomErr::SyntaxError("Unsupported type".to_string())),
        };

        Ok(col_type)
    }
}

// CREATE INDEX
#[derive(Debug, PartialEq)]
pub struct CreateIndexStmt {
    name: String,
    table: String,
    column: String
}

impl Eq for CreateIndexStmt {}

impl CreateIndexStmt {
    pub fn new(name: String, table: String, column: String) -> Self {
        Self { name, table, column }
    }

    get_attr_str!(name);
    get_attr_str!(table);
    get_attr_str!(column);
}

impl Build for CreateIndexStmt {
    fn build(tokens: &mut Tokens) -> Result<Option<Self>, CustomErr> {
        let mut target_tokens = Some(vec![Token::Create { start: 0 }]);
        let mut builder = CreateIndexStmtBuilder::new();

        loop {
            let Some(token) = tokens.current() else {
                return Err(tokens.get_syntax_error())
            };

            println!("Token {:?}", &token);
            if !token.is_token_type_matched(&target_tokens) {
                return Err(tokens.get_syntax_error())
            };

            match token {
                Token::Create { .. } => target_tokens = Some(vec![Token::Index { start: 0 }]),
                Token::Index { .. } => target_tokens = Some(vec![Token::Ident { start: 0, value: "" }]),
                Token::Ident { value, .. } => {
                    builder.with_name(value.to_string());
                    target_tokens = Some(vec![Token::On { start: 0 }]);
                },
                Token::On { .. } => {
                    let Some(table_token) = tokens.current() else {
                        return Err(tokens.get_syntax_error())
                    };

                    match table_token {
                        Token::Ident { value, .. } => builder.with_table(value.to_string()),
                        _ => return Err(tokens.get_syntax_error()),
                    };

                    let Some(lparen) = tokens.current() else {
                        return Err(tokens.get_syntax_error())
                    };
                    if !matches!(lparen, Token::Lparen { .. }) {
                        return Err(tokens.get_syntax_error())
                    };

                    let Some(column_token) = tokens.current() else {
                        return Err(tokens.get_syntax_error())
                    };
                    match column_token {
                        Token::Ident { value, .. } => builder.with_column(value.to_string()),
                        _ => return Err(tokens.get_syntax_error())
                    };
                    break;
                },
                _ => return Err(tokens.get_syntax_error())
            }
        };
        Ok(Some(builder.build()?))
    }
}

// Builder for CreateIndexStmt
macro_rules! index_builder_attr {
    ($field:ident) => {
        paste::paste! {
            pub fn [<with_ $field>](&mut self, $field: String) -> &mut Self {
                self.$field = Some($field);
                self
            }
        }
    };
}

struct CreateIndexStmtBuilder {
    name: Option<String>,
    table: Option<String>,
    column: Option<String>,
}

impl CreateIndexStmtBuilder {
    pub fn new() -> Self {
        Self { name: None, table: None, column: None }
    }

    index_builder_attr!(name);
    index_builder_attr!(table);
    index_builder_attr!(column);

    pub fn build(self) -> Result<CreateIndexStmt, CustomErr> {
        let Some(name) = self.name else {
            return Err(CustomErr::SyntaxError("Index name must be provided".to_string()))
        };

        let Some(table) = self.table else {
            return Err(CustomErr::SyntaxError("Table name must be provided".to_string()))
        };

        let Some(column) = self.column else {
            return Err(CustomErr::SyntaxError("Column name must be provided".to_string()))
        };

        Ok(CreateIndexStmt::new(name, table, column))
    }
}

#[cfg(test)]
mod tests {
    use crate::parser::tokenizer::tokenize;

    use super::*;

    #[test]
    fn test_create_tbl_stmt() {
        let s = "CREATE TABLE companies(\
        id integer primary key autoincrement, name text, \
        domain text, year_founded text, industry text, \"size range\" text, \
        locality text, country text, current_employees text, total_employees text)";

        let mut tokens = tokenize(&s).unwrap();
        let create_stmt = CreateTableStmt::build(&mut tokens);
        println!("Create table stmt: {:?}", &create_stmt);
        assert!(create_stmt.is_ok());
    }

    #[test]
    fn test_create_index_stmt() {
        let s = "CREATE INDEX idx_companies_country on companies (country)";

        let mut tokens = tokenize(&s).unwrap();
        let create_stmt = CreateIndexStmt::build(&mut tokens);
        println!("Create index stmt: {:?}", &create_stmt);
        assert!(create_stmt.is_ok());
    }
}
