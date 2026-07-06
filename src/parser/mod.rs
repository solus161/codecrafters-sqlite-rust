pub mod tokenizer;
pub mod create;
pub mod select;

use crate::{btree::CellPayload, exceptions::CustomErr, parser::tokenizer::tokenize};
use tokenizer::{ Tokens, Token };
use create::{ CreateTableStmt, CreateIndexStmt };
use select::SelectStmt;

// A general trait for all AST node
pub trait Build: Sized {
    // The AST owns its strings, so it is
    // independent of both the token slice and the original SQL source.
    fn build(tokens: &mut Tokens) -> Result<Option<Self>, CustomErr>;
}

// A general trait for all expr that could be evaluated
pub trait Eval {
    fn eval(&self, row: &CellPayload) -> Result<EvalOutput, CustomErr>;
}

// Output of eval
pub enum EvalOutput {
    Bool(bool),
    Array,      // This is subquery, leave it for later
}

impl EvalOutput {
    pub fn is_true(&self) -> bool {
        match self {
            Self::Bool(b) => *b,
            _ => false
        }
    }
}

#[derive(Debug, PartialEq)]
pub enum Ast {
    CreateTable(CreateTableStmt),
    CreateIndex(CreateIndexStmt),
    Select(SelectStmt),
}

impl Build for Ast {
    fn build(tokens: &mut Tokens) -> Result<Option<Self>, CustomErr> {
        // Check first 2 tokens for type
        let Some(first_token) = tokens.peek(0) else {
            return Ok(None)
        };

        match first_token {
            Token::Create { .. } => {
                let Some(second_token) = tokens.peek(1) else {
                    return Err(tokens.get_syntax_error())
                };

                match second_token {
                    Token::Table { .. } => {
                        let stmt = CreateTableStmt::build(tokens)?
                            .ok_or(CustomErr::Internal)?;
                        Ok(Some(Self::CreateTable(stmt)))
                    },
                    Token::Index { .. } => {
                        let stmt = CreateIndexStmt::build(tokens)?
                            .ok_or(CustomErr::Internal)?;
                        Ok(Some(Self::CreateIndex(stmt)))
                    },
                    _ => return Err(tokens.get_syntax_error())
                }
            },
            Token::Select { .. } => {
                let stmt = SelectStmt::build(tokens)?
                    .ok_or(CustomErr::Internal)?;
                Ok(Some(Self::Select(stmt)))
            },
            _ => return Err(tokens.get_syntax_error())
        }
    }
}

pub fn ast_from_str(s: &str) -> Result<Option<Ast>, CustomErr> {
    let mut tokens = tokenize(s)?;
    let ast = Ast::build(&mut tokens)?;
    Ok(ast)
}

#[cfg(test)]
mod tests {
    use crate::parser::tokenizer::tokenize;

    use super::*;

    #[test]
    fn test_create_tbl_ast() {
        let s = "CREATE TABLE companies(\
        id integer primary key autoincrement, name text, \
        domain text, year_founded text, industry text, \"size range\" text, \
        locality text, country text, current_employees text, total_employees text)";

        let mut tokens = tokenize(&s).unwrap();
        let ast = Ast::build(&mut tokens);
        println!("Create table AST: {:?}", &ast);
        assert!(ast.is_ok());
    }

    #[test]
    fn test_create_index_ast() {
        let s = "CREATE INDEX idx_companies_country on companies (country)";

        let mut tokens = tokenize(&s).unwrap();
        let ast = Ast::build(&mut tokens);
        println!("Create index AST: {:?}", &ast);
        assert!(ast.is_ok());
    }

    #[test]
    fn test_select_stmt() {
        let s = " select name, \"company size\", count(*), count(\"more name\"), count ( age ) \
                from table1 \
                where (a = 'a' or (b > 5 and b1 = 1)) or c = 'c' and d <= 10 ";

        let mut tokens = tokenize(&s).unwrap();
        let ast = Ast::build(&mut tokens);
        println!("Select AST: {:?}", &ast);
        assert!(ast.is_ok());
    }
}
