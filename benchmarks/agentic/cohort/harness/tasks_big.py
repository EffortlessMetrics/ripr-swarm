# Larger tasks: mutants are generated mechanically (mutate.py) and filtered by killability.
BIG = {}
def big(id, msg, base_src, new_src, weak_tests):
    BIG[id] = dict(id=id, msg=msg, base_src=base_src, new_src=new_src, weak_tests=weak_tests, mutants=None, ref_tests="", base_tests="")

big("calendar", "feat: date arithmetic (add_days, weekday, diff, parse)",
base_src='''#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Date { pub year: i32, pub month: u32, pub day: u32 }
''',
new_src='''#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Date { pub year: i32, pub month: u32, pub day: u32 }

pub fn is_leap(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

pub fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => if is_leap(year) { 29 } else { 28 },
        _ => 0,
    }
}

impl Date {
    pub fn new(year: i32, month: u32, day: u32) -> Option<Date> {
        if month < 1 || month > 12 || day < 1 || day > days_in_month(year, month) {
            return None;
        }
        Some(Date { year, month, day })
    }

    pub fn next_day(self) -> Date {
        if self.day < days_in_month(self.year, self.month) {
            Date { day: self.day + 1, ..self }
        } else if self.month < 12 {
            Date { month: self.month + 1, day: 1, ..self }
        } else {
            Date { year: self.year + 1, month: 1, day: 1 }
        }
    }

    pub fn prev_day(self) -> Date {
        if self.day > 1 {
            Date { day: self.day - 1, ..self }
        } else if self.month > 1 {
            let m = self.month - 1;
            Date { month: m, day: days_in_month(self.year, m), ..self }
        } else {
            Date { year: self.year - 1, month: 12, day: 31 }
        }
    }

    pub fn add_days(self, n: i64) -> Date {
        let mut d = self;
        if n >= 0 {
            for _ in 0..n { d = d.next_day(); }
        } else {
            for _ in 0..(-n) { d = d.prev_day(); }
        }
        d
    }

    /// 0 = Monday .. 6 = Sunday (Sakamoto's algorithm).
    pub fn weekday(self) -> u32 {
        const T: [i32; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
        let y = if self.month < 3 { self.year - 1 } else { self.year };
        let w = (y + y / 4 - y / 100 + y / 400 + T[(self.month - 1) as usize] + self.day as i32) % 7;
        ((w + 6) % 7) as u32
    }

    pub fn parse(s: &str) -> Option<Date> {
        let parts: Vec<&str> = s.split('-').collect();
        if parts.len() != 3 || parts[0].len() != 4 || parts[1].len() != 2 || parts[2].len() != 2 {
            return None;
        }
        let y = parts[0].parse::<i32>().ok()?;
        let m = parts[1].parse::<u32>().ok()?;
        let d = parts[2].parse::<u32>().ok()?;
        Date::new(y, m, d)
    }

    pub fn is_weekend(self) -> bool { self.weekday() >= 5 }
}
''',
weak_tests='''use calendar::*;
#[test]
fn leap_basic() { assert!(is_leap(2024)); assert!(!is_leap(2023)); }
#[test]
fn next_day_simple() { assert_eq!(Date::new(2023, 5, 10).unwrap().next_day(), Date { year: 2023, month: 5, day: 11 }); }
#[test]
fn parse_ok() { assert_eq!(Date::parse("2024-03-05"), Date::new(2024, 3, 5)); }
''')

big("expr", "feat: integer expression evaluator with precedence and errors",
base_src='''pub fn add(a: i64, b: i64) -> i64 { a + b }
''',
new_src='''#[derive(Debug, PartialEq, Eq)]
pub enum EvalError { Syntax(usize), DivByZero, Overflow }

struct P<'a> { s: &'a [u8], i: usize }

impl<'a> P<'a> {
    fn ws(&mut self) { while self.i < self.s.len() && self.s[self.i] == b' ' { self.i += 1; } }
    fn peek(&mut self) -> Option<u8> { self.ws(); self.s.get(self.i).copied() }

    fn expr(&mut self) -> Result<i64, EvalError> {
        let mut v = self.term()?;
        while let Some(c) = self.peek() {
            if c != b'+' && c != b'-' { break; }
            self.i += 1;
            let r = self.term()?;
            v = if c == b'+' { v.checked_add(r) } else { v.checked_sub(r) }.ok_or(EvalError::Overflow)?;
        }
        Ok(v)
    }

    fn term(&mut self) -> Result<i64, EvalError> {
        let mut v = self.power()?;
        while let Some(c) = self.peek() {
            if c != b'*' && c != b'/' && c != b'%' { break; }
            self.i += 1;
            let r = self.power()?;
            v = match c {
                b'*' => v.checked_mul(r).ok_or(EvalError::Overflow)?,
                _ => {
                    if r == 0 { return Err(EvalError::DivByZero); }
                    if c == b'/' { v.checked_div(r).ok_or(EvalError::Overflow)? } else { v.rem_euclid(r) }
                }
            };
        }
        Ok(v)
    }

    fn power(&mut self) -> Result<i64, EvalError> {
        let base = self.unary()?;
        if self.peek() == Some(b'^') {
            self.i += 1;
            let exp = self.power()?;
            if exp < 0 { return Err(EvalError::Syntax(self.i)); }
            let e = u32::try_from(exp).map_err(|_| EvalError::Overflow)?;
            return base.checked_pow(e).ok_or(EvalError::Overflow);
        }
        Ok(base)
    }

    fn unary(&mut self) -> Result<i64, EvalError> {
        if self.peek() == Some(b'-') {
            self.i += 1;
            let v = self.unary()?;
            return v.checked_neg().ok_or(EvalError::Overflow);
        }
        self.atom()
    }

    fn atom(&mut self) -> Result<i64, EvalError> {
        match self.peek() {
            Some(b'(') => {
                self.i += 1;
                let v = self.expr()?;
                if self.peek() != Some(b')') { return Err(EvalError::Syntax(self.i)); }
                self.i += 1;
                Ok(v)
            }
            Some(c) if c.is_ascii_digit() => {
                let start = self.i;
                while self.i < self.s.len() && self.s[self.i].is_ascii_digit() { self.i += 1; }
                std::str::from_utf8(&self.s[start..self.i]).unwrap().parse::<i64>().map_err(|_| EvalError::Overflow)
            }
            _ => Err(EvalError::Syntax(self.i)),
        }
    }
}

pub fn eval(input: &str) -> Result<i64, EvalError> {
    let mut p = P { s: input.as_bytes(), i: 0 };
    let v = p.expr()?;
    if p.peek().is_some() { return Err(EvalError::Syntax(p.i)); }
    Ok(v)
}
pub fn add(a: i64, b: i64) -> i64 { a + b }
''',
weak_tests='''use expr::*;
#[test]
fn adds() { assert_eq!(eval("1 + 2"), Ok(3)); }
#[test]
fn precedence_basic() { assert_eq!(eval("2 + 3 * 4"), Ok(14)); }
#[test]
fn div_zero() { assert_eq!(eval("1 / 0"), Err(EvalError::DivByZero)); }
''')

big("textfmt", "feat: CSV line parser and greedy word wrap",
base_src='''pub fn lines(s: &str) -> Vec<&str> { s.lines().collect() }
''',
new_src='''/// Split one CSV record. Fields may be quoted; "" inside quotes is a literal quote.
/// Returns None for an unterminated quote or text after a closing quote.
pub fn parse_csv_line(line: &str) -> Option<Vec<String>> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_q = false;
    let mut after_q = false;
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if in_q {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    cur.push('"');
                    chars.next();
                } else {
                    in_q = false;
                    after_q = true;
                }
            } else {
                cur.push(c);
            }
        } else if c == ',' {
            out.push(std::mem::take(&mut cur));
            after_q = false;
        } else if c == '"' && cur.is_empty() && !after_q {
            in_q = true;
        } else if after_q {
            return None;
        } else {
            cur.push(c);
        }
    }
    if in_q {
        return None;
    }
    out.push(cur);
    Some(out)
}

/// Greedy wrap at `width` chars. A word longer than `width` goes on its own line unbroken.
pub fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut lines: Vec<String> = Vec::new();
    let mut cur = String::new();
    for word in text.split_whitespace() {
        let wl = word.chars().count();
        let cl = cur.chars().count();
        if cur.is_empty() {
            cur.push_str(word);
        } else if cl + 1 + wl <= width {
            cur.push(' ');
            cur.push_str(word);
        } else {
            lines.push(std::mem::take(&mut cur));
            cur.push_str(word);
        }
    }
    if !cur.is_empty() {
        lines.push(cur);
    }
    lines
}

pub fn lines(s: &str) -> Vec<&str> { s.lines().collect() }
''',
weak_tests='''use textfmt::*;
#[test]
fn csv_simple() { assert_eq!(parse_csv_line("a,b,c"), Some(vec!["a".into(), "b".into(), "c".into()])); }
#[test]
fn csv_quoted() { assert_eq!(parse_csv_line("\\"x,y\\",z"), Some(vec!["x,y".into(), "z".into()])); }
#[test]
fn wrap_basic() { assert_eq!(wrap("aaa bbb ccc", 7), vec!["aaa bbb".to_string(), "ccc".to_string()]); }
''')
