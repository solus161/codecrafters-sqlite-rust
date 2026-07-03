use crate::exceptions::CustomErr;
use super::tokenizer::{Token, Tokens, tokenize};
use super::{Build};

// Trait for node that can be evaluated into bool
// WhereCondition or WhereOperator node
pub trait EvalBool {}

/* Examples of SELECT, we will not implement all
    -- Basic
    SELECT statement could have following forms:
    SELECT name FROM countries
    SELECT name, gdp FROM countries
    SELECT name, gdp, population FROM countries
    SELECT *, name FROM countries

    -- Column expression, we stop here, but the data must have places for advanced cases
    SELECT COUNT(*) FROM countries
    SELECT COUNT(*) AS total FROM countries
    SELECT MAX(gdp), MIN(gdp) FROM countries
    SELECT UPPER(name) FROM countries

    -- Advanced column expresstion
    SELECT price * quantity FROM orders
    SELECT price * quantity AS total FROM orders
    SELECT gdp / population AS gdp_per_capita FROM countries
    SELECT 1 + 1 FROM countries          -- valid, constant expression repeated per row
    
    -- Join SELECT * FROM countries, cities                          -- implicit cross join
    SELECT * FROM countries JOIN cities ON countries.id = cities.country_id
    SELECT * FROM countries INNER JOIN cities ON ...
    SELECT * FROM countries LEFT JOIN cities ON ...
    SELECT * FROM countries AS c JOIN cities AS ci ON c.id = ci.country_id

    -- Subquery
    SELECT * FROM (SELECT name, gdp FROM countries WHERE gdp > 1000) AS rich_countries
 */

// Select statement, consiste of 3 clauses
#[derive(Debug, PartialEq)]
pub struct SelectStmt {
    pub select_clause: SelectClause,
    pub from_clause: FromClause,
    pub where_clause: Option<WhereClause> 
}

impl Build for SelectStmt {
    fn build(tokens: &mut Tokens) -> Result<Option<Self>, CustomErr> {
        let select_clause = SelectClause::build(tokens)?
            .ok_or(CustomErr::Internal)?;  
        let from_clause = FromClause::build(tokens)?
            .ok_or(CustomErr::Internal)?;
        let where_clause = WhereClause::build(tokens)?;
        Ok(Some(Self {
            select_clause,
            from_clause,
            where_clause
        }))
    }
}

// 1st element of Select stmt
#[derive(Debug, PartialEq)]
pub struct SelectClause(Vec<ColumnExpr>);

impl Eq for SelectClause {}

impl Build for SelectClause {
    fn build(tokens: &mut Tokens) -> Result<Option<Self>, CustomErr>
    {
        if tokens.is_empty() {
            return Err(CustomErr::SyntaxError("Nothing provided".to_string()))
        };

        let mut columns: Vec<ColumnExpr> = Vec::new();
        let mut target_tokens: Option<Vec<Token>> = Some(vec![Token::Select{ start: 0 }]);

        // let mut tokens_iter = tokens.iter().enumerate();

        loop {
            // If current token is From, break do not consume
            let Some(current_token) = tokens.peek(0) else {
                break
            };
            if matches!(current_token, Token::From { .. }) {
                break
            };

            let Some(token) = tokens.current() else { break };
            if !token.is_token_type_matched(&target_tokens) {
                return Err(tokens.get_syntax_error()) 
            };

            match token {
                Token::Select { .. } => {
                    target_tokens = Some(vec![
                        Token::Ident{ start: 0, value: "" },
                        Token::Count{ start: 0}
                    ])
                },
                Token::Ident { value, .. }=> {
                    columns.push(ColumnExpr::Name(value.to_string()));
                    target_tokens = Some(vec![
                        Token::Comma { start: 0 },
                        Token::From { start: 0 }
                    ]);
                },
                Token::Count { .. } => {
                    // Extract following token right here
                    // LPAREN
                    let Some(lparen) = tokens.current() else {
                        return Err(tokens.get_syntax_error())
                    };

                    if !matches!(lparen, Token::Lparen { .. }) {
                        return Err(tokens.get_syntax_error())
                    };

                    // Ident or asterik
                    let Some(ident) = tokens.current() else {
                        return Err(tokens.get_syntax_error())
                    };
                    
                    let column_expr = match ident {
                        Token::Ident { value, .. } => ColumnExpr::Name(value.to_string()),
                        Token::Asterik { .. } => ColumnExpr::All,
                        _ => return Err(tokens.get_syntax_error()),
                    };

                    // RPAREN
                    let Some(rparen) = tokens.current() else {
                        return Err(tokens.get_syntax_error())
                    };

                    if !matches!(rparen, Token::Rparen { .. }) {
                        return Err(tokens.get_syntax_error())
                    };

                    target_tokens = Some(vec![
                        Token::Comma { start: 0 }, 
                        Token::From { start: 0 }
                    ]);
                    columns.push(ColumnExpr::Count(Box::new(column_expr)));
                },
                Token::Comma{ .. } => {
                    target_tokens = Some(vec![
                        Token::Ident { start: 0, value: "" },
                        Token::Count { start: 0 }
                    ]);
                },
                _ => return Err(tokens.get_syntax_error())
            }
        };

        Ok(Some(Self(columns)))
    }
}

// Name or expression that could be evaluated to a column
// Used in Select stmt
#[derive(Debug, PartialEq)]
pub enum ColumnExpr {
    Name(String),
    Count(Box<ColumnExpr>),
    All
}

impl Eq for ColumnExpr {}

// 2nd element of Select stmt
#[derive(Debug, PartialEq)]
pub struct FromClause(TableExpr);

impl Eq for FromClause {}

impl Build for FromClause {
    fn build(tokens: &mut Tokens) -> Result<Option<Self>, CustomErr> {
        if tokens.is_empty() {
            return Err(CustomErr::SyntaxError("Nothing provided".to_string()))
        };

        let mut target_tokens: Option<Vec<Token>> = Some(vec![Token::From { start: 0 }]);
        let table_expr: Option<TableExpr>;

        loop {
            let Some(token) = tokens.current() else {
                return Err(tokens.get_syntax_error())
            };

            if !token.is_token_type_matched(&target_tokens) {
                return Err(tokens.get_syntax_error()) 
            };
            match token {
                Token::From { .. } => target_tokens = Some(vec![Token::Ident { start: 0, value: "" }]),
                Token::Ident { value: name, .. } => {
                    table_expr = Some(TableExpr::Name(name.to_string()));
                    break;
                },
                _ => return Err(tokens.get_syntax_error())
            }
        };

        // If the current token is EoF, must consume that
        if let Some(eof) = tokens.peek(0) {
            if matches!(eof, Token::EoF { .. }) {
                let _eof = tokens.current();
            };
        };

        Ok(Some(FromClause(table_expr.expect("Must not be None")))) 
    }
}

// Name or expression evaluated to table
#[derive(Debug, PartialEq)]
pub enum TableExpr {
    Name(String),
}

impl Eq for TableExpr {}

// 3rd element of Select stmt
/* The Where clause could be even more complicated:
We do not cover all
    WHERE age = 30
    WHERE name = 'Bob'
    WHERE price > 19.99
    WHERE active != 0
    WHERE age > 18 AND age < 65
    WHERE country = 'US' OR country = 'CA'
    WHERE age > 18 AND country = 'US' OR vip = 1000
    WHERE (age > 18 AND country = 'US') OR vip = 1
    WHERE age > 18 AND (country = 'US' OR country = 'CA')
    WHERE NOT active
    WHERE NOT (age > 18 AND country = 'US')
    WHERE middle_name IS NULL
    WHERE middle_name IS NOT NULL
    WHERE age BETWEEN 18 AND 65
    WHERE country IN ('US', 'CA', 'MX')
    WHERE id IN (1, 2, 3)
    WHERE id IN (SELECT user_id FROM banned_users)
    WHERE name LIKE 'A%'
    WHERE name NOT LIKE '%son'
    WHERE gdp > (SELECT AVG(gdp) FROM countries)
    WHERE id IN (SELECT user_id FROM orders WHERE total > 100)
    WHERE EXISTS (SELECT 1 FROM orders WHERE orders.user_id = users.id)
    WHERE LENGTH(name) > 10
    WHERE UPPER(country) = 'US'
    WHERE users.id = orders.user_id
*/
#[derive(Debug, PartialEq)]
pub struct WhereClause(WhereExpr);

impl Eq for WhereClause {}

impl Build for WhereClause {
    fn build(tokens: &mut Tokens) -> Result<Option<Self>, CustomErr> {
        if tokens.is_empty() {
            return Err(CustomErr::SyntaxError("Nothing provided".to_string()))
        };

        let Some(current_token) = tokens.peek(0) else {
            return Ok(None)
        };

        if !matches!(current_token, Token::Where { .. }) {
            return Err(tokens.get_syntax_error())
        };

        // Move next
        let Some(_token) = tokens.current() else {
            return Err(tokens.get_syntax_error())
        };

        let Some(expr) = WhereExpr::build(tokens)? else {
            return Err(tokens.get_syntax_error())
        };

        Ok(Some(Self(expr)))
    }
}

// A Where node could be either condition or operator
#[derive(Debug, PartialEq)]
pub enum WhereExpr {
    Condition(WhereCondition),
    Operator(Box<WhereOperator>),
}

impl Eq for WhereExpr {}

impl Build for WhereExpr {
    fn build(tokens: &mut Tokens) -> Result<Option<Self>, CustomErr> {
        // This could be build recursively due to ( )
        if tokens.is_empty() {
            return Err(CustomErr::SyntaxError("Nothing provided".to_string()))
        };
        
        // WHERE is detected by parent stack frame
        // After WHERE, accept only Ident and ()
        let mut target_tokens: Option<Vec<Token>> = Some(vec![
            Token::Ident { start: 0, value: "" },
            Token::Lparen { start: 0 }
        ]);
        // let mut tokens_iter = tokens.iter().enumerate();

        let mut expr_stack: Vec<WhereExpr> = Vec::new();
        let mut op_stack: Vec<LogicOp> = Vec::new();
        loop {
            // Before move to next token, must peek to current to identify )
            // as WhereExpr could be nested
            let Some(current_token) = tokens.peek(0) else {
                // Nothing next
                break 
            };

            if matches!(current_token, Token::Rparen { .. }) {
                // Next token is ), no need to move next, return
                break 
            };

            // Current token is not ), could move next
            let Some(token) = tokens.current() else { break };
            if !token.is_token_type_matched(&target_tokens) {
                return Err(tokens.get_syntax_error()) 
            };

            match token {
                Token::Ident { value: name, .. } => {
                    // Check next 2 tokens must be opeator and operand
                    let Some(op) = tokens.current() else {
                        return Err(tokens.get_syntax_error())
                    };

                    
                    let Some(operand) = tokens.current() else {
                        return Err(tokens.get_syntax_error())
                    };
                    
                    let op = CompSpecOp::try_from(op)?;
                    let value: ValueExpr = match operand {
                        Token::StrLiteral { value, .. } => ValueExpr::Text((*value).to_string()),
                        Token::IntLiteral { value, .. } => ValueExpr::Integer(*value),
                        Token::FloatLiteral { value, .. } => ValueExpr::Float(*value),
                        _ => return Err(tokens.get_syntax_error())
                    };

                    let cond = WhereCondition{
                        column_exp: ColumnExpr::Name(name.to_string()),
                        op: op,
                        target: value
                    };

                    if expr_stack.is_empty() && op_stack.is_empty() {
                        expr_stack.push(WhereExpr::Condition(cond));
                    } else if !expr_stack.is_empty() && !op_stack.is_empty() {
                        // We got a node here, form new node only if top op is AND
                        let Some(top_op) = op_stack.last() else {
                            return Err(CustomErr::Internal) 
                        };
                        if matches!(top_op, LogicOp::And) {
                            let node_left = expr_stack.pop().expect("Expr stack must not be empty");
                            let op = op_stack.pop().expect("Op stack must not be empty");
                            let node = WhereOperator{
                                left: node_left,
                                op: op,
                                right: WhereExpr::Condition(cond)
                            };
                            expr_stack.push(WhereExpr::Operator(Box::new(node)));
                        } else {
                            // If top logic op is OR, we push cond to expr_stack
                            // and only build node with next cond/expr and AND logic
                            expr_stack.push(WhereExpr::Condition(cond))
                        }
                    } else {
                        // A cond cannot follow a cond/expr
                        return Err(tokens.get_syntax_error())
                    };
                    target_tokens = None;
                },
                Token::And { .. } | Token::Or { .. } => {
                    if expr_stack.is_empty() {
                        // AND/OR must follow a cond or and expr
                        return Err(tokens.get_syntax_error())
                    };

                    let op = LogicOp::try_from(token)?;
                    op_stack.push(op);
                    target_tokens = Some(vec![
                        Token::Lparen { start: 0 },
                        Token::Ident { start: 0, value: "" }
                    ])
                },
                Token::Lparen { .. } => {
                    // Recursively here
                    let Some(expr) = WhereExpr::build(tokens)? else {
                        return Err(tokens.get_syntax_error())
                    };
                    
                    // Current token must be ), that's what terminates the expr
                    let Some(rparen) = tokens.current() else {
                        // Lparen does not having matched Rparen
                        return Err(tokens.get_syntax_error())
                    };

                    if !matches!(rparen, Token::Rparen { .. }) {
                        // St terminates the expr
                        return Err(tokens.get_syntax_error())
                    };
                    
                    // Ok got the expr
                    if expr_stack.is_empty() && op_stack.is_empty() {
                        expr_stack.push(expr);
                    } else if !expr_stack.is_empty() && !op_stack.is_empty() {
                        // We got a node here, form new node only if top op is AND
                        let Some(top_op) = op_stack.last() else {
                            return Err(CustomErr::Internal) 
                        };

                        if matches!(top_op, LogicOp::And) {
                            let node_left = expr_stack.pop().expect("Expr stack must not be empty");
                            let op = op_stack.pop().expect("Op stack must not be empty");
                            let node = WhereOperator{
                                left: node_left,
                                op: op,
                                right: expr
                            };
                            expr_stack.push(WhereExpr::Operator(Box::new(node)));
                        } else {
                            // If top logic op is OR, we push cond to expr_stack
                            // and only build node with next cond/expr and AND logic
                            expr_stack.push(expr)
                        }
                    } else {
                        // A cond/expr cannot follow a cond/expr
                        return Err(tokens.get_syntax_error())
                    };
                    target_tokens = None;
                },
                _ => return Err(tokens.get_syntax_error())
            }
        };
        
        // Ok, now the expr_stack need to be resolve
        // println!("Expr stack {:?}", &expr_stack);
        // println!("Op stack {:?}", &op_stack);

        while expr_stack.len() > 1 {
            let Some(node_right) = expr_stack.pop() else {
                return Err(CustomErr::Internal)
            };
            let Some(op) = op_stack.pop() else {
                return Err(CustomErr::Internal)
            };
            let Some(node_left) = expr_stack.pop() else {
                return Err(CustomErr::Internal)
            };
            let node = WhereOperator{
                left: node_left,
                op: op,
                right: node_right
            };
            let expr = WhereExpr::Operator(Box::new(node));
            expr_stack.push(expr);
        }
        Ok(Some(expr_stack.pop().ok_or(CustomErr::Internal)?))
    }
}

// This is a kind of tree
#[derive(Debug, PartialEq)]
pub struct WhereOperator {
    left: WhereExpr,
    op: LogicOp,
    right: WhereExpr,
}

impl Eq for WhereOperator {}

impl EvalBool for WhereOperator {}

// This is leaf of the above tree
#[derive(Debug, PartialEq)]
pub struct WhereCondition {
    column_exp: ColumnExpr,
    op: CompSpecOp,
    target: ValueExpr
}

// Comparison and special operators
#[derive(Debug, PartialEq)]
pub enum CompSpecOp {
    Eq,
    Neq,
    Gt,
    Lt,
    Gte,
    Lte,
}

impl Eq for CompSpecOp {}

impl TryFrom<&Token<'_>> for CompSpecOp {
    type Error = CustomErr;

    fn try_from(value: &Token) -> Result<Self, Self::Error> {
        match value {
            Token::Eq { .. } => Ok(Self::Eq),
            Token::Neq { .. }=> Ok(Self::Neq),
            Token::Gt { .. } => Ok(Self::Gt),
            Token::Lt { .. } => Ok(Self::Lt),
            Token::Gte { .. } => Ok(Self::Gte),
            Token::Lte { .. } => Ok(Self::Lte),
            _ => Err(CustomErr::SyntaxError("Unsupported operator".to_string()))
        } 
    }
}

// Logical operator
#[derive(Debug, PartialEq)]
pub enum LogicOp {
    And,
    Or
}

impl Eq for LogicOp {}

impl TryFrom<&Token<'_>> for LogicOp {
    type Error = CustomErr;

    fn try_from(value: &Token) -> Result<Self, Self::Error> {
        match value {
            Token::And { .. } => Ok(Self::And),
            Token::Or { .. } => Ok(Self::Or),
            _ => Err(CustomErr::SyntaxError("Unsupported operator".to_string()))
        }
    }
}

// Parenthese
struct Lparen;

#[derive(Debug, PartialEq)]
pub enum ValueExpr {
    Text(String),
    Integer(i64),
    Float(f64)
}

impl Eq for ValueExpr {}

// Testing
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_select() {
        let s = " select name, \"company size\", count(*), count(\"more name\"), count ( age ) ";
        let target = SelectClause(
            vec![
                ColumnExpr::Name("name".to_string()),
                ColumnExpr::Name("company size".to_string()),
                ColumnExpr::Count(Box::new(ColumnExpr::All)),
                ColumnExpr::Count(Box::new(ColumnExpr::Name("more name".to_string()))),
                ColumnExpr::Count(Box::new(ColumnExpr::Name("age".to_string())))
            ]
        );

        let mut tokens = tokenize(&s).unwrap();
        let select_clause = SelectClause::build(&mut tokens);
        println!("Select clause: {:?}", &select_clause);
        assert!(select_clause.is_ok());
        assert_eq!(select_clause.unwrap().unwrap(), target);
    }

    #[test]
    fn test_build_from() {
        let s = "  from table1 ";
        let target = FromClause(TableExpr::Name("table1".to_string()));
        let mut tokens = tokenize(&s).unwrap();
        let from_clause = FromClause::build(&mut tokens);
        println!("From clause: {:?}", &from_clause);
        assert!(from_clause.is_ok());
        assert_eq!(from_clause.unwrap().unwrap(), target);
    }

    #[test]
    fn test_build_where() {
        let s = " where a = 'a' and b > 5 or  \"location\" <= 5 ";
        // let target = WhereClause(
        //     WhereExpr::Operator(Box::new(
        //             WhereOperator {
        //                 left: WhereExpr::Condition(
        //                           WhereCondition { column_exp: j, op: (), target: () }
        //                       ), op: (), right: () }
        //     ))
        // );
        let mut tokens = tokenize(&s).unwrap();
        let where_clause = WhereClause::build(&mut tokens);
        // println!("Where clause: {:?}", &where_clause);
        assert!(where_clause.is_ok());
        // assert_eq!(from_clause.unwrap().unwrap(), target);
    }

    #[test]
    fn test_build_where1() {
        let s = " where (a = 'a' or (b > 5 and b1 = 1)) or c = 'c' and d <= 10 ";
        // let target = WhereClause(
        //     WhereExpr::Operator(Box::new(
        //             WhereOperator {
        //                 left: WhereExpr::Condition(
        //                           WhereCondition { column_exp: j, op: (), target: () }
        //                       ), op: (), right: () }
        //     ))
        // );
        let mut tokens = tokenize(&s).unwrap();
        let where_clause = WhereClause::build(&mut tokens);
        println!("Where clause: {:?}", &where_clause);
        assert!(where_clause.is_ok());
        // assert_eq!(from_clause.unwrap().unwrap(), target);
    }

    #[test]
    fn test_build_select_stmt() {
        let s = " select name, \"company size\", count(*), count(\"more name\"), count ( age ) \
                from table1 \
                where (a = 'a' or (b > 5 and b1 = 1)) or c = 'c' and d <= 10 ";
        let mut tokens = tokenize(&s).unwrap();
        let select_stmt = SelectStmt::build(&mut tokens);
        println!("Select statement: {:?}", &select_stmt);
        assert!(select_stmt.is_ok());
    }
}
