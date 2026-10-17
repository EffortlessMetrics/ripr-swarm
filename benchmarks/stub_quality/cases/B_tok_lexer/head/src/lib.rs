use std::fmt;

pub const MAX_NUMBER_LEN: usize = 8;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Token {
    Num(u64),
    Ident(String),
    Plus,
    Minus,
    Star,
    LParen,
    RParen,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LexError {
    UnexpectedChar(char, usize),
    NumberTooLong(usize),
}

impl fmt::Display for LexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LexError::UnexpectedChar(c, at) => write!(f, "unexpected {c:?} at {at}"),
            LexError::NumberTooLong(at) => write!(f, "number too long at {at}"),
        }
    }
}

impl std::error::Error for LexError {}

pub fn tokenize(src: &str) -> Result<Vec<Token>, LexError> {
    let bytes = src.as_bytes();
    let mut i = 0;
    let mut out = Vec::new();
    while i < bytes.len() {
        let c = bytes[i] as char;
        match c {
            ' ' | '\t' => i += 1,
            '+' => {
                out.push(Token::Plus);
                i += 1;
            }
            '-' => {
                out.push(Token::Minus);
                i += 1;
            }
            '*' => {
                out.push(Token::Star);
                i += 1;
            }
            '(' => {
                out.push(Token::LParen);
                i += 1;
            }
            ')' => {
                out.push(Token::RParen);
                i += 1;
            }
            '0'..='9' => {
                let start = i;
                while i < bytes.len() && bytes[i].is_ascii_digit() {
                    i += 1;
                }
                if i - start >= MAX_NUMBER_LEN {
                    return Err(LexError::NumberTooLong(start));
                }
                let n = src[start..i].parse::<u64>().unwrap_or(0);
                out.push(Token::Num(n));
            }
            c if c.is_ascii_alphabetic() || c == '_' => {
                let start = i;
                while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                    i += 1;
                }
                out.push(Token::Ident(src[start..i].to_string()));
            }
            other => return Err(LexError::UnexpectedChar(other, i)),
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simple_expr() {
        let t = tokenize("1 + 23").unwrap();
        assert_eq!(t, vec![Token::Num(1), Token::Plus, Token::Num(23)]);
    }

    #[test]
    fn idents_and_parens() {
        let t = tokenize("(foo_1)").unwrap();
        assert_eq!(t.len(), 3);
        assert_eq!(t[1], Token::Ident("foo_1".to_string()));
    }

    #[test]
    fn long_number_rejected() {
        assert!(tokenize("123456789012345678901234567890").is_err());
    }

    #[test]
    fn unexpected_char() {
        assert!(matches!(tokenize("1 $ 2"), Err(LexError::UnexpectedChar('$', 2))));
    }
}
