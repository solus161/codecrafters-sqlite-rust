use std::mem::discriminant;
use std::cell::Cell;
use std::panic::Location;

use crate::exceptions::CustomErr;

const KW_DBINFO: &str = ".dbinfo";
const KW_SELECT: &str = "SELECT";
const KW_ASTERIK: &str = "*";
const KW_FROM: &str = "FROM";
const KW_LPAREN: &str = "(";
const KW_RPAREN: &str = ")";
const KW_WHERE: &str = "WHERE";
const KW_EQ: &str = "=";
const KW_APOSTROPHE: &str = "'";
const KW_CREATE: &str = "CREATE";
const KW_TABLE: &str = "TABLE";
const KW_INTEGER: &str = "INTEGER";
const KW_TEXT: &str = "TEXT";
const KW_PRIMARY_KEY: &str = "PRIMARY KEY";
const KW_PRIMARY: &str = "PRIMARY";
const KW_AUTO_INCREMENT: &str = "AUTOINCREMENT";
const KW_QUOTATION: &str = "\"";
const KW_INDEX: &str = "INDEX";
const KW_ON: &str = "ON";
const KW_COUNT: &str = "COUNT";
const KW_AND: &str = "AND";
const KW_OR: &str = "OR";
const KW_NOT: &str = "NOT";
const KW_IN: &str = "IN";
const KW_LIKE: &str = "LIKE";
const KW_ILIKE: &str = "ILIKE";
const KW_BETWEEN: &str = "BETWEEN";
const KW_NULL: &str = "NULL";

#[derive(Debug, PartialEq)]
pub struct Tokens<'a> {
    tokens: Vec<Token<'a>>,
    src: &'a str,
    cur: Cell<usize>,
    init: bool,
}

impl<'a> Tokens<'a> {
    pub fn new(tokens: Vec<Token<'a>>, src: &'a str) -> Self {
        Self { tokens, src, cur: Cell::new(0), init: false }
    }

    pub fn current(&self) -> Option<&Token<'a>> {
        // Return ref to current token and move cursor up 1 item
        let current = self.tokens.get(self.cur.get());
        self.cur.update(|x| x + 1);
        current
    }

    // pub fn next(&self, n: usize) {
    //     // Move cursor to next n items
    //     self.cur.update(|x| x + n);
    // }

    pub fn peek(&self, n: usize) -> Option<&Token<'a>> {
        // Peek to next n item
        //
        self.tokens.get(self.cur.get() + n)
    }

    pub fn is_empty(&self) -> bool {
        self.tokens.is_empty()
    }

    pub fn cur(&self) -> usize {
        self.cur.get()
    }
    
    #[track_caller]
    pub fn get_syntax_error(&self) -> CustomErr {
        let Some(current_token) = self.peek(0) else {
            return CustomErr::Tokenize("Token at position 0 not found".to_string());
        };

        let start = match current_token {
            Token::Select { start } | Token::Create { start } |
                Token::Table { start } | Token::Index { start } |
                Token::From { start } | Token::On { start } |
                Token::In { start } | Token::Comma { start } |
                Token::Eq { start } | Token::Ne { start } |
                Token::Gt { start } | Token::Lt { start } |
                Token::Ge { start } | Token::Le { start } |
                Token::And { start } | Token::Or { start } |
                Token::Count { start } | Token::Lparen { start } | Token::Rparen { start } |
                Token::Asterik { start } | Token::Where { start } |
                Token::Integer { start } | Token::Text { start } |
                Token::AutoIncrement { start } | Token::PrimaryKey { start } |
                Token::Semicolon { start } | Token::Not { start } | Token::Null { start } |
                Token::Like { start } | Token::Ilike { start } |
                Token::Between { start } | Token::Float { start } |
                Token::EoF { start } => {
                *start 
            },
            Token::Ident { start, .. } | Token::StrLiteral { start, .. } => {
                *start 
            },
            Token::IntLiteral { start, .. } | Token::FloatLiteral { start, .. } => {
                *start
            }
        };
        
        let loc = Location::caller();
        let msg = format!(
            "Syntax error near {:?} (at {}:{})",
            &self.src[..start+1],
            loc.file(),
            loc.line());
        CustomErr::SyntaxError(msg) 
    }
}

#[derive(Debug, PartialEq)]
pub enum Token<'a> {
    Select{ start: usize },
    Count{ start: usize },
    Asterik{ start: usize },
    From{ start: usize },
    Comma{ start: usize },
    Lparen{ start: usize },
    Rparen{ start: usize },
    Where{ start: usize },
    Eq{ start: usize },
    Ne{ start: usize},
    Gt{ start: usize },
    Lt{ start: usize },
    Ge{ start: usize },
    Le{ start: usize },
    Ident{ start: usize, value: &'a str },
    StrLiteral{ start: usize, value: &'a str },
    IntLiteral{ start: usize, value: i64 },
    FloatLiteral{ start: usize, value: f64 },
    Create{ start: usize },
    Table{ start: usize },
    Integer{ start: usize },
    Text{ start: usize },
    Float{ start: usize },
    PrimaryKey{ start: usize },
    AutoIncrement{ start: usize },
    Index{ start: usize },
    On{ start: usize },
    Semicolon{ start: usize },
    And{ start: usize },
    Or{ start: usize },
    EoF{ start: usize },
    Not{ start: usize },
    In{ start: usize },
    Like{ start: usize },
    Ilike{ start: usize },
    Between{ start: usize },
    Null{ start: usize }
}

impl<'a> Eq for Token<'a> {}

impl<'a> Token<'a> {
    pub fn is_token_type_matched(&self, target_tokens: &Option<Vec<Self>>) -> bool {
        match target_tokens {
            None => return true,
            Some(v) => {
                for t in v {
                    if discriminant(self) == discriminant(t) {
                        return true
                    }
                }
            },
        };
        return false
    }
}

fn skip_spaces(chars: &Vec<char>, i: &mut usize) {
    while *i < chars.len() {
        if chars[*i].is_whitespace() {
            *i += 1;
            continue;
        };
        break
    }
}

fn peek(chars: &Vec<char>, i: usize) -> Option<&char> {
    chars.get(i)
}

fn next_to_delimiter(s: &str) -> usize {
    s.find(|c: char|
        c.is_whitespace() || c == ',' || c == '(' || c == ')' || c == '\'' ||
        c == '"' || c == '=' || c == '!' || c == '>' || c == '<' || c == ';')
        .unwrap_or(s.len())
}

fn next_to_ascii(chars: &Vec<char>, from: usize) -> usize {
    let mut i = from;
    while i <= chars.len() {
        if chars[i].is_ascii() { return i - from };
        i += 1
    };
    i - from
}

fn next_to_az(chars: &Vec<char>, from: usize) -> usize {
    let mut i = from;
    while i <= chars.len() {
        if chars[i].is_alphabetic() { return i - from };
        i += 1
    };
    i - from
}

fn get_tokenize_err(s: &str) -> CustomErr {
    CustomErr::Tokenize(format!("Syntax error near {}", s))
}

pub fn tokenize<'a>(s: &'a str) -> Result<Tokens<'a>, CustomErr> {
    if s.is_empty() {
        return Ok(Tokens::new(Vec::new(), s))
    };

    let mut i: usize = 0;
    let chars: Vec<char> = s.chars().into_iter().collect();
    let mut output: Vec<Token> = Vec::new();
    while i < s.len() {
        match chars[i] {
            c if c.is_whitespace() => {
                // 
                skip_spaces(&chars, &mut i);
            },
            ',' => { output.push(Token::Comma{ start: i }); i += 1 },
            '(' => { output.push(Token::Lparen{ start: i }); i += 1},
            ')' => { output.push(Token::Rparen{ start: i }); i += 1},
            '=' => { output.push(Token::Eq{ start: i }); i += 1 },
            '!' => {
                let Some(c) = peek(&chars, i + 1) else {
                    return Err(get_tokenize_err(&s[..i+1]))};

                if *c == '=' {
                    output.push(Token::Ne{ start: i });
                    i += 2;
                } else {
                    return Err(get_tokenize_err(&s[..i+1]))
                }
            },
            '>' => {
                let Some(c) = peek(&chars, i + 1) else {
                    output.push(Token::Gt{ start: i });
                    i += 1;
                    continue;
                };

                if *c == '=' {
                    i += 2;
                    output.push(Token::Ge{ start: i });
                } else {
                    i += 1;
                    output.push(Token::Gt{ start: i });
                };
            },
            '<' => {
                let Some(c) = peek(&chars, i + 1) else {
                    output.push(Token::Lt{ start: i });
                    i += 1;
                    continue;
                };

                if *c == '=' {
                    i += 2;
                    output.push(Token::Le{ start: i });
                } else {
                    i += 1;
                    output.push(Token::Lt{ start: i });
                };
            },
            '"' => {
                // Ident here, as quotation is used
                i += 1;
                let Some(next_quotation) = s[i..].find('"') else {
                    return Err(get_tokenize_err(&s[..i+1]))
                };

                output.push(
                    Token::Ident{
                        start: i,
                        value: &s[i..i + next_quotation]
                    });
                i += next_quotation + 1;
                println!()
            },
            '\'' => {
                // String literal here
                i += 1;
                let Some(next_apos) = s[i..].find('\'') else {
                    return Err(get_tokenize_err(&s[..i+1]))
                };

                output.push(
                    Token::StrLiteral{
                        start: i,
                        value: &s[i..i + next_apos]
                    });
                i += next_apos + 1;
            },
            ';' => { output.push(Token::EoF{ start: i }); i += 1 },
            c if c.is_numeric() => {
                let next_delimiter = next_to_delimiter(&s[i..]);
                let value = &s[i..i + next_delimiter];
                if let Ok(x) = value.parse::<i64>() {
                    i += next_delimiter;
                    output.push(Token::IntLiteral{ start: i, value: x });
                } else if let Ok(x) = value.parse::<f64>() {
                    i += next_delimiter;
                    output.push(Token::FloatLiteral{ start: i, value:x })
                } else {
                    return Err(get_tokenize_err(&s[..i+1]))
                }
            },
            c if c.is_ascii() => {
                let next_delimiter = next_to_delimiter(&s[i..]);
                if next_delimiter == 0 {
                    return Err(get_tokenize_err(&s[..i+1]))
                };
                
                let token = match s[i..i + next_delimiter].to_ascii_uppercase().as_str() {
                    KW_SELECT => Token::Select{ start: i },
                    KW_ASTERIK => Token::Asterik{ start: i },
                    KW_FROM => Token::From{ start: i },
                    KW_PRIMARY => {
                        let next_az = next_to_az(&chars, i + next_delimiter);
                        let j = i + next_delimiter + next_az;
                        let next_delimiter_j = next_to_delimiter(&s[j..]);

                        if &s[j..j + next_delimiter_j].to_ascii_uppercase() == "KEY" {
                            i += next_az + next_delimiter_j;
                            Token::PrimaryKey{ start: i }
                        } else {
                            return Err(get_tokenize_err(&s[..i+next_delimiter]))
                        }
                    },
                    KW_WHERE => Token::Where{ start: i },
                    KW_CREATE => Token::Create{ start: i },
                    KW_TABLE => Token::Table{ start: i },
                    KW_ON => Token::On{ start: i },
                    KW_INTEGER => Token::Integer{ start: i },
                    KW_TEXT => Token::Text{ start: i },
                    KW_AUTO_INCREMENT => Token::AutoIncrement{ start: i },
                    KW_INDEX => Token::Index{ start: i },
                    KW_COUNT => Token::Count{ start: i },
                    KW_AND => Token::And{ start: i },
                    KW_OR => Token::Or{ start: i },
                    KW_NOT => Token::Not{ start: i },
                    KW_IN => Token::In{ start: i },
                    KW_LIKE => Token::Like{ start: i },
                    KW_ILIKE => Token::Ilike{ start: i },
                    KW_BETWEEN => Token::Between{ start: i },
                    KW_NULL => Token::Null { start: i },
                    _ => {
                        
                        Token::Ident{ start: i, value: &s[i..i + next_delimiter]}
                    }
                };
                i += next_delimiter;
                output.push(token); 
            },
            _ => return Err(get_tokenize_err(&s[..i+1]))
        }

    };
    
    let tokens = Tokens::new(output, s);
    Ok(tokens)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenize_01() {
        let s = "  CREATE  TABLE   companies(\
        id integer primary  key  autoincrement\n, name text, domain text,\
        year_founded text,industry text  ,  \"size range\" text  ,\
        locality text, country   text  , current_employees text, total_employees text) ;";

        let tokenize_output = tokenize(s);
        let target: Vec<Token> = vec![
            Token::Create{ start: 2 }, Token::Table{ start: 10 },
            Token::Ident{ start: 18, value: "companies" }, Token::Lparen{ start: 27 },
            Token::Ident{ start: 28, value: "id" }, Token::Integer{ start: 31 },
            Token::PrimaryKey{ start: 44 }, Token::AutoIncrement{ start: 53 },
            Token::Comma{ start: 67 },
            Token::Ident{ start: 69, value: "name" }, Token::Text{ start: 74 },
            Token::Comma{ start: 78 },
            Token::Ident{ start: 80, value: "domain" }, Token::Text{ start: 87 },
            Token::Comma{ start: 91 },
            Token::Ident{ start: 92, value: "year_founded"}, Token::Text{ start: 105 },
            Token::Comma{ start: 109 },
            Token::Ident{ start: 110, value: "industry" }, Token::Text{ start: 119 },
            Token::Comma{ start: 125 },
            Token::Ident{ start: 129, value: "size range" }, Token::Text{ start: 141 },
            Token::Comma{ start: 147 },
            Token::Ident{ start: 148, value: "locality" }, Token::Text{ start: 157 },
            Token::Comma{ start: 161 },
            Token::Ident{ start: 163, value: "country" }, Token::Text{ start: 173 },
            Token::Comma{ start: 179 },
            Token::Ident{ start: 181, value: "current_employees" }, Token::Text{ start: 199 },
            Token::Comma{ start: 203 },
            Token::Ident{ start: 205, value: "total_employees" }, Token::Text{ start: 221 },
            Token::Rparen{ start: 225 }, Token::EoF{ start: 227 }
        ];
        assert!(tokenize_output.is_ok());
        assert_eq!(tokenize_output.unwrap().tokens, target);
    }
    
    #[test]
    fn test_tokenize_02() {
        let s = "CREATE INDEX idx_companies_country    on companies(country)";
        let tokenize_output = tokenize(s);
        let target = vec![
            Token::Create{ start: 0 }, Token::Index{ start: 7 },
            Token::Ident{ start: 13, value: "idx_companies_country" },
            Token::On{ start: 38 }, Token::Ident{ start: 41, value: "companies" },
            Token::Lparen{ start: 50 },
            Token::Ident{ start: 51, value: "country" }, Token::Rparen{ start: 58 }
        ];
        assert!(tokenize_output.is_ok());
        assert_eq!(tokenize_output.unwrap().tokens, target);
    }

    #[test]
    fn test_tokenize_03() {
        let s = "select *, \"test column\", count(*)\
                 from companies where status >= 1 and or \"country\"='vietnam';";
        let tokenize_output = tokenize(s);
        let target = vec![
            Token::Select{ start: 0 }, Token::Asterik{ start: 7 },
            Token::Comma{ start: 8 },
            Token::Ident{ start: 11, value: "test column" },
            Token::Comma{ start: 23 },
            Token::Count{ start: 25 }, Token::Lparen{ start: 30 },
            Token::Asterik{ start: 31 }, Token::Rparen{ start: 32 },
            Token::From{ start: 33 }, Token::Ident{ start: 38, value: "companies" },
            Token::Where{ start: 48 },
            Token::Ident{ start: 54, value: "status" }, Token::Ge{ start: 63 },
            Token::IntLiteral{ start: 65, value: 1 },
            Token::And{ start: 66 }, Token::Or{ start: 70 },
            Token::Ident{ start: 74, value: "country" }, Token::Eq{ start: 82 },
            Token::StrLiteral{ start: 84, value: "vietnam" },
            Token::EoF{ start: 92 }
        ];
        assert!(tokenize_output.is_ok());
        assert_eq!(tokenize_output.unwrap().tokens, target);
    }

}
