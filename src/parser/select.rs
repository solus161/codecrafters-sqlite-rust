use super::tokenizer::{Token, Tokens, tokenize};
use super::{Build, Eval, EvalOutput};
use crate::btree::{BTree, CellColValue, CellPayload, Table};
use crate::exceptions::CustomErr;

// Trait for node that can be evaluated into bool
// WhereCondition or WhereOperator node
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
    pub where_clause: Option<WhereClause>,
}

impl Build for SelectStmt {
    fn build(tokens: &mut Tokens) -> Result<Option<Self>, CustomErr> {
        let select_clause = SelectClause::build(tokens)?;
        let from_clause = FromClause::build(tokens)?;
        let where_clause = WhereClause::build(tokens)?;
        Ok(Some(Self {
            select_clause: select_clause.expect("Select clause must not be None"),
            from_clause: from_clause.expect("From clause mut not be None"),
            where_clause,
        }))
    }
}

impl SelectStmt {
    pub fn eval_where(&self, row: &CellPayload) -> Result<bool, CustomErr> {
        let Some(where_clause) = &self.where_clause else {
            return Ok(true);
        };
        let eval_output = where_clause.eval(row)?;
        Ok(eval_output.is_true())
    }

    pub fn resolve(&mut self, table: &Table) -> Result<(), CustomErr> {
        // Resolve column name in where to index in payload
        match &mut self.where_clause {
            Some(where_clause) => where_clause.resolve(table),
            None => Ok(()),
        }
    }

    pub fn resolve_index(&self) -> Option<IndexCondition> {
        // Simple filtering by index plan
        // Return an index together with filter instruction
        // If the lvl1 of WhereExpr is a condition
        // or is A AND B with either A or B is a condition
        let Some(where_clause) = &self.where_clause else {
            // return None
            return None;
        };
        where_clause.resolve_index()
    }
}

// 1st element of Select stmt
#[derive(Debug, PartialEq)]
pub struct SelectClause(Vec<ColumnExpr>);

impl Eq for SelectClause {}

impl Build for SelectClause {
    fn build(tokens: &mut Tokens) -> Result<Option<Self>, CustomErr> {
        if tokens.is_empty() {
            return Err(CustomErr::SyntaxError("Nothing provided".to_string()));
        };

        let mut columns: Vec<ColumnExpr> = Vec::new();
        let mut target_tokens: Option<Vec<Token>> = Some(vec![Token::Select { start: 0 }]);

        // let mut tokens_iter = tokens.iter().enumerate();

        while let Some(current_token) = tokens.peek(0)
            && !matches!(current_token, Token::From { .. })
        {
            // If current token is From, break do not consume
            // let Some(current_token) = tokens.peek(0) else {
            //     break
            // };
            // if matches!(current_token, Token::From { .. }) {
            //     break
            // };

            let Some(token) = tokens.current() else { break };
            if !token.is_token_type_matched(&target_tokens) {
                return Err(tokens.get_syntax_error());
            };

            match token {
                Token::Select { .. } => {
                    target_tokens = Some(vec![
                        Token::Ident {
                            start: 0,
                            value: "",
                        },
                        Token::Count { start: 0 },
                        Token::Asterik { start: 0 },
                    ])
                }
                Token::Ident { value, .. } => {
                    columns.push(ColumnExpr::Name {
                        name: value.to_string(),
                        index: None,
                    });
                    target_tokens = Some(vec![Token::Comma { start: 0 }, Token::From { start: 0 }]);
                }
                Token::Count { .. } => {
                    // Extract following token right here
                    // LPAREN
                    let Some(lparen) = tokens.current() else {
                        return Err(tokens.get_syntax_error());
                    };

                    if !matches!(lparen, Token::Lparen { .. }) {
                        return Err(tokens.get_syntax_error());
                    };

                    // Ident or asterik
                    let Some(ident) = tokens.current() else {
                        return Err(tokens.get_syntax_error());
                    };

                    let column_expr = match ident {
                        Token::Ident { value, .. } => ColumnExpr::Name {
                            name: value.to_string(),
                            index: None,
                        },
                        Token::Asterik { .. } => ColumnExpr::All,
                        _ => return Err(tokens.get_syntax_error()),
                    };

                    // RPAREN
                    let Some(rparen) = tokens.current() else {
                        return Err(tokens.get_syntax_error());
                    };

                    if !matches!(rparen, Token::Rparen { .. }) {
                        return Err(tokens.get_syntax_error());
                    };

                    target_tokens = Some(vec![Token::Comma { start: 0 }, Token::From { start: 0 }]);
                    columns.push(ColumnExpr::Count(Box::new(column_expr)));
                }
                Token::Asterik { .. } => {
                    target_tokens = Some(vec![Token::Comma { start: 0 }, Token::From { start: 0 }]);
                }
                Token::Comma { .. } => {
                    target_tokens = Some(vec![
                        Token::Ident {
                            start: 0,
                            value: "",
                        },
                        Token::Count { start: 0 },
                    ]);
                }
                _ => return Err(tokens.get_syntax_error()),
            }
        }

        Ok(Some(Self(columns)))
    }
}

impl SelectClause {
    pub fn columns(&self) -> &[ColumnExpr] {
        self.0.as_ref()
    }
}

// Name or expression that could be evaluated to a column
// Used in Select stmt
#[derive(Debug, PartialEq)]
pub enum ColumnExpr {
    Name { name: String, index: Option<u64> },
    Count(Box<ColumnExpr>),
    All,
}

impl Eq for ColumnExpr {}

impl ColumnExpr {
    pub fn resolve(&mut self, table: &Table) -> Result<(), CustomErr> {
        if let Self::Name { name, index } = self {
            let col_index = table
                .column_index(name.as_str())
                .ok_or(CustomErr::Execution("Invalid column name".to_string()))?;
            *index = Some(*col_index);
        };
        Ok(())
    }

    pub fn index(&self) -> Option<u64> {
        match self {
            Self::Name { index, .. } => *index,
            _ => None,
        }
    }

    pub fn get_column(&self) -> Option<&str> {
        match self {
            Self::Name { name, .. } => Some(name.as_str()),
            _ => None,
        }
    }
}

// 2nd element of Select stmt
#[derive(Debug, PartialEq)]
pub struct FromClause(TableExpr);

impl Eq for FromClause {}

impl FromClause {
    pub fn table_name(&self) -> Option<&str> {
        self.0.name()
    }
}

impl Build for FromClause {
    fn build(tokens: &mut Tokens) -> Result<Option<Self>, CustomErr> {
        if tokens.is_empty() {
            return Err(CustomErr::SyntaxError("Nothing provided".to_string()));
        };

        let mut target_tokens: Option<Vec<Token>> = Some(vec![Token::From { start: 0 }]);
        let table_expr: Option<TableExpr>;

        loop {
            let Some(token) = tokens.current() else {
                return Err(tokens.get_syntax_error());
            };

            if !token.is_token_type_matched(&target_tokens) {
                return Err(tokens.get_syntax_error());
            };
            match token {
                Token::From { .. } => {
                    target_tokens = Some(vec![Token::Ident {
                        start: 0,
                        value: "",
                    }])
                }
                Token::Ident { value: name, .. } => {
                    table_expr = Some(TableExpr::Name(name.to_string()));
                    break;
                }
                _ => return Err(tokens.get_syntax_error()),
            }
        }

        // If the current token is EoF, must consume that
        if let Some(eof) = tokens.peek(0)
            && matches!(eof, Token::EoF { .. })
        {
            let _eof = tokens.current();
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

impl TableExpr {
    pub fn name(&self) -> Option<&str> {
        match self {
            Self::Name(s) => Some(s.as_str()),
        }
    }
}

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
            return Err(CustomErr::SyntaxError("Nothing provided".to_string()));
        };

        let Some(current_token) = tokens.peek(0) else {
            return Ok(None);
        };

        if !matches!(current_token, Token::Where { .. }) {
            return Err(tokens.get_syntax_error());
        };

        // Move next
        let Some(_token) = tokens.current() else {
            return Err(tokens.get_syntax_error());
        };

        let Some(expr) = WhereExpr::build(tokens)? else {
            return Err(tokens.get_syntax_error());
        };

        Ok(Some(Self(expr)))
    }
}

impl WhereClause {
    fn resolve(&mut self, table: &Table) -> Result<(), CustomErr> {
        self.0.resolve(table)
    }

    fn resolve_index(&self) -> Option<IndexCondition> {
        self.0.resolve_index()
    }
}

impl Eval for WhereClause {
    fn eval(&self, row: &CellPayload) -> Result<EvalOutput, CustomErr> {
        self.0.eval(row)
    }
}

// A Where node could be either condition or operator
#[derive(Debug, PartialEq)]
pub enum WhereExpr {
    Condition(WhereCondition),
    Operator(Box<WhereOperator>),
}

impl Eq for WhereExpr {}

impl WhereExpr {
    pub fn resolve(&mut self, table: &Table) -> Result<(), CustomErr> {
        // Add index based on column name
        // return column name used, help with walking later
        match self {
            Self::Condition(cond) => cond.resolve(table),
            Self::Operator(box_op) => box_op.resolve(table),
        }
    }

    pub fn resolve_index(&self) -> Option<IndexCondition> {
        match self {
            Self::Condition(cond) => IndexCondition::from_where_cond(cond),
            Self::Operator(box_op) => match box_op.op {
                LogicOp::And => {
                    let c1 = box_op.left.resolve_index();
                    if c1.is_none() {
                        box_op.right.resolve_index()
                    } else {
                        c1
                    }
                }
                _ => None,
            },
        }
    }
}

impl Build for WhereExpr {
    fn build(tokens: &mut Tokens) -> Result<Option<Self>, CustomErr> {
        // This could be build recursively due to ( )
        if tokens.is_empty() {
            return Err(CustomErr::SyntaxError("Nothing provided".to_string()));
        };

        // WHERE is detected by parent stack frame
        // After WHERE, accept only Ident and ()
        let mut target_tokens: Option<Vec<Token>> = Some(vec![
            Token::Ident {
                start: 0,
                value: "",
            },
            Token::Lparen { start: 0 },
        ]);
        // let mut tokens_iter = tokens.iter().enumerate();

        let mut expr_stack: Vec<WhereExpr> = Vec::new();
        let mut op_stack: Vec<LogicOp> = Vec::new();
        while let Some(current_token) = tokens.peek(0)
            && !matches!(current_token, Token::Rparen { .. } | Token::EoF { .. })
        {
            // Before move to next token, must peek to current to identify )
            // as WhereExpr could be nested
            // let Some(current_token) = tokens.peek(0) else {
            //     // Nothing next
            //     break
            // };
            //
            // if matches!(current_token, Token::Rparen { .. } | Token::EoF { .. }) {
            //     // Next token is ) or end-of-statement (;), stop without consuming
            //     break
            // };

            // Current token is not ), could move next
            let Some(token) = tokens.current() else { break };
            if !token.is_token_type_matched(&target_tokens) {
                return Err(tokens.get_syntax_error());
            };

            match token {
                Token::Ident { value: name, .. } => {
                    // Check next 2 tokens must be opeator and operand
                    let Some(op) = tokens.current() else {
                        return Err(tokens.get_syntax_error());
                    };

                    let Some(operand) = tokens.current() else {
                        return Err(tokens.get_syntax_error());
                    };

                    let op = CompSpecOp::try_from(op)?;
                    let value: ValueExpr = match operand {
                        Token::StrLiteral { value, .. } => ValueExpr::Text((*value).to_string()),
                        Token::IntLiteral { value, .. } => ValueExpr::Integer(*value),
                        Token::FloatLiteral { value, .. } => ValueExpr::Float(*value),
                        _ => return Err(tokens.get_syntax_error()),
                    };

                    let cond = WhereCondition {
                        column_expr: ColumnExpr::Name {
                            name: name.to_string(),
                            index: None,
                        },
                        op,
                        target: value,
                    };

                    if expr_stack.is_empty() && op_stack.is_empty() {
                        expr_stack.push(WhereExpr::Condition(cond));
                    } else if !expr_stack.is_empty() && !op_stack.is_empty() {
                        // We got a node here, form new node only if top op is AND
                        let Some(top_op) = op_stack.last() else {
                            return Err(CustomErr::BuildAST(
                                "Op stack mut not be empty".to_string(),
                            ));
                        };
                        if matches!(top_op, LogicOp::And) {
                            let node_left = expr_stack.pop().expect("Expr stack must not be empty");
                            let op = op_stack.pop().expect("Op stack must not be empty");
                            let node = WhereOperator {
                                left: node_left,
                                op,
                                right: WhereExpr::Condition(cond),
                            };
                            expr_stack.push(WhereExpr::Operator(Box::new(node)));
                        } else {
                            // If top logic op is OR, we push cond to expr_stack
                            // and only build node with next cond/expr and AND logic
                            expr_stack.push(WhereExpr::Condition(cond))
                        }
                    } else {
                        // A cond cannot follow a cond/expr
                        return Err(tokens.get_syntax_error());
                    };
                    target_tokens = None;
                }
                Token::And { .. } | Token::Or { .. } => {
                    if expr_stack.is_empty() {
                        // AND/OR must follow a cond or and expr
                        return Err(tokens.get_syntax_error());
                    };

                    let op = LogicOp::try_from(token)?;
                    op_stack.push(op);
                    target_tokens = Some(vec![
                        Token::Lparen { start: 0 },
                        Token::Ident {
                            start: 0,
                            value: "",
                        },
                    ])
                }
                Token::Lparen { .. } => {
                    // Recursively here
                    let Some(expr) = WhereExpr::build(tokens)? else {
                        return Err(tokens.get_syntax_error());
                    };

                    // Current token must be ), that's what terminates the expr
                    let Some(rparen) = tokens.current() else {
                        // Lparen does not having matched Rparen
                        return Err(tokens.get_syntax_error());
                    };

                    if !matches!(rparen, Token::Rparen { .. }) {
                        // St terminates the expr
                        return Err(tokens.get_syntax_error());
                    };

                    // Ok got the expr
                    if expr_stack.is_empty() && op_stack.is_empty() {
                        expr_stack.push(expr);
                    } else if !expr_stack.is_empty() && !op_stack.is_empty() {
                        // We got a node here, form new node only if top op is AND
                        let top_op = op_stack.last().expect("Op stack must not be empty");

                        if matches!(top_op, LogicOp::And) {
                            let node_left = expr_stack.pop().expect("Expr stack must not be empty");
                            let op = op_stack.pop().expect("Op stack must not be empty");
                            let node = WhereOperator {
                                left: node_left,
                                op,
                                right: expr,
                            };
                            expr_stack.push(WhereExpr::Operator(Box::new(node)));
                        } else {
                            // If top logic op is OR, we push cond to expr_stack
                            // and only build node with next cond/expr and AND logic
                            expr_stack.push(expr)
                        }
                    } else {
                        // A cond/expr cannot follow a cond/expr
                        return Err(tokens.get_syntax_error());
                    };
                    target_tokens = None;
                }
                _ => return Err(tokens.get_syntax_error()),
            }
        }

        // Ok, now the expr_stack need to be resolve
        // println!("Expr stack {:?}", &expr_stack);
        // println!("Op stack {:?}", &op_stack);

        while expr_stack.len() > 1 {
            let node_right = expr_stack.pop().expect("Node right not found");
            let op = op_stack.pop().expect("Op not found");
            let node_left = expr_stack.pop().expect("Node left not found");
            let node = WhereOperator {
                left: node_left,
                op,
                right: node_right,
            };
            let expr = WhereExpr::Operator(Box::new(node));
            expr_stack.push(expr);
        }
        Ok(Some(expr_stack.pop().expect("Expr stack mut not be empty")))
    }
}

impl Eval for WhereExpr {
    fn eval(&self, row: &CellPayload) -> Result<EvalOutput, CustomErr> {
        let eval_output = match self {
            Self::Condition(cond) => cond.eval(row)?,
            Self::Operator(box_op) => box_op.eval(row)?,
        };
        Ok(EvalOutput::Bool(eval_output.is_true()))
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

impl Eval for WhereOperator {
    fn eval(&self, row: &CellPayload) -> Result<EvalOutput, CustomErr> {
        let left_bool = self.left.eval(row)?;
        let right_bool = self.right.eval(row)?;
        let output_bool = match self.op {
            LogicOp::And => left_bool.is_true() && right_bool.is_true(),
            LogicOp::Or => left_bool.is_true() | right_bool.is_true(),
        };
        Ok(EvalOutput::Bool(output_bool))
    }
}

impl WhereOperator {
    pub fn resolve(&mut self, table: &Table) -> Result<(), CustomErr> {
        self.left.resolve(table)?;
        self.right.resolve(table)?;
        Ok(())
    }
}

// This is leaf of the above tree
#[derive(Debug, PartialEq)]
pub struct WhereCondition {
    column_expr: ColumnExpr,
    op: CompSpecOp,
    target: ValueExpr,
}

impl WhereCondition {
    fn resolve(&mut self, table: &Table) -> Result<(), CustomErr> {
        self.column_expr.resolve(table)
    }

    pub fn get_column(&self) -> Option<&str> {
        self.column_expr.get_column()
    }
}

impl Eval for WhereCondition {
    fn eval(&self, row: &CellPayload) -> Result<EvalOutput, CustomErr> {
        let index = self
            .column_expr
            .index()
            .ok_or(CustomErr::Execution("Invalid column index".to_string()))?;
        let value_expr = ValueExpr::try_from(row.column(index as usize).ok_or(
            CustomErr::Execution(format!("Column index {} not found", index)),
        )?)?;

        let bool_value = match self.op {
            CompSpecOp::Eq => value_expr == self.target,
            CompSpecOp::Ne => value_expr != self.target,
            CompSpecOp::Gt => value_expr > self.target,
            CompSpecOp::Lt => value_expr < self.target,
            CompSpecOp::Ge => value_expr >= self.target,
            CompSpecOp::Le => value_expr <= self.target,
        };

        Ok(EvalOutput::Bool(bool_value))
    }
}

// Comparison and special operators
#[derive(Debug, PartialEq, Clone)]
pub enum CompSpecOp {
    Eq,
    Ne,
    Gt,
    Lt,
    Ge,
    Le,
}

impl Eq for CompSpecOp {}

impl TryFrom<&Token<'_>> for CompSpecOp {
    type Error = CustomErr;

    fn try_from(value: &Token) -> Result<Self, Self::Error> {
        match value {
            Token::Eq { .. } => Ok(Self::Eq),
            Token::Ne { .. } => Ok(Self::Ne),
            Token::Gt { .. } => Ok(Self::Gt),
            Token::Lt { .. } => Ok(Self::Lt),
            Token::Ge { .. } => Ok(Self::Ge),
            Token::Le { .. } => Ok(Self::Le),
            _ => Err(CustomErr::SyntaxError("Unsupported operator".to_string())),
        }
    }
}

// Logical operator
#[derive(Debug, PartialEq, Clone)]
pub enum LogicOp {
    And,
    Or,
}

impl Eq for LogicOp {}

impl TryFrom<&Token<'_>> for LogicOp {
    type Error = CustomErr;

    fn try_from(value: &Token) -> Result<Self, Self::Error> {
        match value {
            Token::And { .. } => Ok(Self::And),
            Token::Or { .. } => Ok(Self::Or),
            _ => Err(CustomErr::SyntaxError("Unsupported operator".to_string())),
        }
    }
}

// Parenthese
struct Lparen;

#[derive(Debug, Clone)]
pub enum ValueExpr {
    Text(String),
    Integer(i64),
    Float(f64),
    Null,
}

impl PartialOrd for ValueExpr {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        // Follow SQLite's storage-class sort order: NULL < numeric < text.
        // The derived order would place Null last (largest), which breaks index
        // traversal because SQLite stores NULL keys first in the b-tree.
        fn rank(v: &ValueExpr) -> u8 {
            match v {
                ValueExpr::Null => 0,
                ValueExpr::Integer(_) | ValueExpr::Float(_) => 1,
                ValueExpr::Text(_) => 2,
            }
        }
        match (self, other) {
            (ValueExpr::Text(a), ValueExpr::Text(b)) => a.partial_cmp(b),
            (ValueExpr::Integer(a), ValueExpr::Integer(b)) => a.partial_cmp(b),
            (ValueExpr::Float(a), ValueExpr::Float(b)) => a.partial_cmp(b),
            (ValueExpr::Integer(a), ValueExpr::Float(b)) => (*a as f64).partial_cmp(b),
            (ValueExpr::Float(a), ValueExpr::Integer(b)) => a.partial_cmp(&(*b as f64)),
            _ => rank(self).partial_cmp(&rank(other)),
        }
    }
}

impl PartialEq for ValueExpr {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Text(a), Self::Text(b)) => a == b,
            (Self::Integer(a), Self::Integer(b)) => a == b,
            (Self::Float(a), Self::Float(b)) => a.to_bits() == b.to_bits(),
            (Self::Null, _) => false,
            (_, Self::Null) => false,
            _ => false,
        }
    }
}

impl TryFrom<&CellColValue> for ValueExpr {
    type Error = CustomErr;

    fn try_from(value: &CellColValue) -> Result<Self, Self::Error> {
        let value_expr = match value {
            CellColValue::Int8(x) => Self::Integer(*x as i64),
            CellColValue::Int16(x) => Self::Integer(*x as i64),
            CellColValue::Int24(x) => Self::Integer(*x as i64),
            CellColValue::Int48(x) => Self::Integer(*x),
            CellColValue::Int64(x) => Self::Integer(*x),
            CellColValue::Float64(x) => Self::Float(*x),
            CellColValue::Text(s) => Self::Text(s.to_string()),
            CellColValue::Null => Self::Null,
            _ => {
                return Err(CustomErr::Execution(
                    "Cannot be evaluated to scalar".to_string(),
                ));
            }
        };
        Ok(value_expr)
    }
}

pub struct IndexCondition {
    column: String,
    op: CompSpecOp,
    target: ValueExpr,
}

impl IndexCondition {
    get_attr_str!(column);

    pub fn op(&self) -> &CompSpecOp {
        &self.op
    }

    pub fn target(&self) -> &ValueExpr {
        &self.target
    }

    pub fn from_where_cond(value: &WhereCondition) -> Option<Self> {
        // An index cannot restrict !=: everything outside a single run matches, so
        // walking the index would visit the whole tree to skip one run. Returning
        // None here lets the planner fall back to a full table scan.
        if matches!(value.op, CompSpecOp::Ne) {
            return None;
        };

        match &value.column_expr {
            ColumnExpr::Name { name, .. } => {
                // Only accept a single column not column expr
                Some(Self {
                    column: name.to_string(),
                    op: value.op.clone(),
                    target: value.target.clone(),
                })
            }
            _ => None,
        }
    }
}

impl Eval for IndexCondition {
    fn eval(&self, row: &CellPayload) -> Result<EvalOutput, CustomErr> {
        // Row is feeded in increasing key order
        let node_col = row.column(0).ok_or(CustomErr::Execution(
            "Index cell has no index value".to_string(),
        ))?; // index value is at index 0 of payload

        let value_expr = ValueExpr::try_from(node_col)?;

        let bool_value = match self.op {
            CompSpecOp::Eq => value_expr == self.target,
            CompSpecOp::Ne => value_expr != self.target,
            CompSpecOp::Gt => value_expr > self.target,
            CompSpecOp::Lt => value_expr < self.target,
            CompSpecOp::Ge => value_expr >= self.target,
            CompSpecOp::Le => value_expr <= self.target,
        };

        Ok(EvalOutput::Bool(bool_value))
    }
}

// impl TryFrom<&WhereCondition> for IndexCondition {
//     type Error = CustomErr;
//
//     fn try_from(value: &WhereCondition) -> Result<Self, Self::Error> {
//
//     }
// }

// Testing
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_select() {
        let s = " select name, \"company size\", count(*), count(\"more name\"), count ( age ) ";
        let target = SelectClause(vec![
            ColumnExpr::Name {
                name: "name".to_string(),
                index: None,
            },
            ColumnExpr::Name {
                name: "company size".to_string(),
                index: None,
            },
            ColumnExpr::Count(Box::new(ColumnExpr::All)),
            ColumnExpr::Count(Box::new(ColumnExpr::Name {
                name: "more name".to_string(),
                index: None,
            })),
            ColumnExpr::Count(Box::new(ColumnExpr::Name {
                name: "age".to_string(),
                index: None,
            })),
        ]);

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
