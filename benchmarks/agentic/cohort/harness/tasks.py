# Task suite: each task = base crate (committed), change commit (new src), weak tests (committed with change),
# hidden mutants of the NEW src (old->new string replacement, must occur exactly once),
# and reference tests used only to validate that every mutant is killable.
TASKS = {}

def task(id, msg, base_src, new_src, weak_tests, mutants, ref_tests, base_tests="", files="lib"):
    TASKS[id] = dict(id=id, msg=msg, base_src=base_src, new_src=new_src, weak_tests=weak_tests,
                     mutants=mutants, ref_tests=ref_tests, base_tests=base_tests)

# ---------------------------------------------------------------- shipping
task("shipping", "feat: tiered shipping with express surcharge",
base_src='''pub fn shipping_cents(weight_g: u32) -> u32 {
    if weight_g <= 500 { 499 } else { 899 }
}
''',
new_src='''/// Standard: <=500g 499c, <=2000g 899c, above that 899c plus 150c per started
/// extra kilogram beyond 2kg. Express adds 50% rounded up to the next cent.
pub fn shipping_cents(weight_g: u32, express: bool) -> u32 {
    let base = if weight_g <= 500 {
        499
    } else if weight_g <= 2000 {
        899
    } else {
        let extra_g = weight_g - 2000;
        899 + 150 * extra_g.div_ceil(1000)
    };
    if express { base + (base + 1) / 2 } else { base }
}
''',
weak_tests='''use shipping::shipping_cents;
#[test]
fn light_parcel_standard() { assert_eq!(shipping_cents(100, false), 499); }
#[test]
fn express_costs_more() { assert!(shipping_cents(100, true) > shipping_cents(100, false)); }
''',
mutants=[
 ("weight_g <= 500", "weight_g < 500"),
 ("extra_g.div_ceil(1000)", "(extra_g / 1000)"),
 ("150 * extra_g", "100 * extra_g"),
 ("899 + 150", "899 - 150"),
 ("(base + 1) / 2", "base / 2"),
 ("base + (base + 1) / 2", "base + (base + 1) / 3"),
 ("if express {", "if !express {"),
],
ref_tests='''use shipping::shipping_cents;
#[test] fn t() {
 assert_eq!(shipping_cents(500,false),499); assert_eq!(shipping_cents(501,false),899);
 assert_eq!(shipping_cents(2000,false),899); assert_eq!(shipping_cents(2001,false),1049);
 assert_eq!(shipping_cents(3000,false),1049); assert_eq!(shipping_cents(3001,false),1199);
 assert_eq!(shipping_cents(100,true),749); assert_eq!(shipping_cents(600,true),1349);
 assert_eq!(shipping_cents(2001,true),1574);
}''')

# ---------------------------------------------------------------- ranges
task("ranges", "feat: merge adjacent and overlapping inclusive ranges",
base_src='''pub fn total_len(ranges: &[(u32, u32)]) -> u32 { ranges.iter().map(|(a, b)| b - a + 1).sum() }
''',
new_src='''/// Merge inclusive ranges that overlap or touch (end + 1 == next start).
pub fn merge_ranges(mut ranges: Vec<(u32, u32)>) -> Vec<(u32, u32)> {
    ranges.sort();
    let mut out: Vec<(u32, u32)> = Vec::new();
    for (start, end) in ranges {
        match out.last_mut() {
            Some(last) if start <= last.1.saturating_add(1) => {
                if end > last.1 {
                    last.1 = end;
                }
            }
            _ => out.push((start, end)),
        }
    }
    out
}
pub fn total_len(ranges: &[(u32, u32)]) -> u32 { ranges.iter().map(|(a, b)| b - a + 1).sum() }
''',
weak_tests='''use ranges::merge_ranges;
#[test]
fn merges_overlap() { assert_eq!(merge_ranges(vec![(1, 5), (3, 8)]), vec![(1, 8)]); }
#[test]
fn keeps_far_apart() { assert_eq!(merge_ranges(vec![(1, 2), (10, 12)]), vec![(1, 2), (10, 12)]); }
''',
mutants=[
 ("start <= last.1.saturating_add(1)", "start <= last.1"),
 ("start <= last.1.saturating_add(1)", "start < last.1"),
 ("last.1.saturating_add(1)", "last.1.saturating_add(2)"),
 ("if end > last.1 {", "if end >= last.1 + 0 && false {"),
 ("ranges.sort();", "ranges.sort_by(|a, b| b.cmp(a)); ranges.reverse(); ranges.sort_by_key(|r| r.1);"),
 ("last.1 = end;", "last.1 = last.1;"),
],
ref_tests='''use ranges::merge_ranges;
#[test] fn t() {
 assert_eq!(merge_ranges(vec![(1,2),(3,4)]), vec![(1,4)]);
 assert_eq!(merge_ranges(vec![(1,2),(4,5)]), vec![(1,2),(4,5)]);
 assert_eq!(merge_ranges(vec![(1,10),(2,3)]), vec![(1,10)]);
 assert_eq!(merge_ranges(vec![(5,6),(1,5)]), vec![(1,6)]);
 assert_eq!(merge_ranges(vec![(1,3),(3,3)]), vec![(1,3)]);
 assert_eq!(merge_ranges(vec![(0,u32::MAX),(5,6)]), vec![(0,u32::MAX)]);
 assert_eq!(merge_ranges(vec![(7,8),(1,2),(2,3)]), vec![(1,3),(7,8)]);
}''')

# ---------------------------------------------------------------- discount
task("discount", "feat: percentage discount with half-up rounding and cap",
base_src='''pub fn price_after(cents: u64, pct: u8) -> u64 { cents - cents * pct as u64 / 100 }
''',
new_src='''/// Apply `pct` percent off. `pct` is capped at 100. The discount amount is
/// rounded half up to a whole cent; the result never exceeds `cents`.
pub fn price_after(cents: u64, pct: u8) -> u64 {
    let pct = pct.min(100) as u64;
    let discount = (cents * pct + 50) / 100;
    cents.saturating_sub(discount)
}
''',
weak_tests='''use discount::price_after;
#[test]
fn ten_percent_off() { assert_eq!(price_after(1000, 10), 900); }
#[test]
fn zero_is_unchanged() { assert_eq!(price_after(1000, 0), 1000); }
''',
mutants=[
 ("(cents * pct + 50) / 100", "(cents * pct) / 100"),
 ("(cents * pct + 50) / 100", "(cents * pct + 49) / 100"),
 ("(cents * pct + 50) / 100", "(cents * pct + 99) / 100"),
 ("cents.saturating_sub(discount)", "cents + discount"),
 ("cents.saturating_sub(discount)", "cents.saturating_sub(discount).max(1)"),
 ("let pct = pct.min(100) as u64;", "let pct = pct.min(50) as u64;"),
 ("(cents * pct + 50) / 100;", "(cents * pct + 50) / 100 + 1;"),
],
ref_tests='''use discount::price_after;
#[test] fn t() {
 assert_eq!(price_after(1000,100), 0); assert_eq!(price_after(1000,255), 0);
 assert_eq!(price_after(1000,101), 0);
 assert_eq!(price_after(5,10), 4); assert_eq!(price_after(15,10), 13); assert_eq!(price_after(1,50), 0);
 assert_eq!(price_after(99,33), 66); assert_eq!(price_after(3,1), 3); assert_eq!(price_after(50,1), 49);
 assert_eq!(price_after(149,1), 148);
}''')

# ---------------------------------------------------------------- duration
task("duration", "feat: parse compound durations like 1h30m15s",
base_src='''pub fn secs(h: u64, m: u64, s: u64) -> u64 { h * 3600 + m * 60 + s }
''',
new_src='''#[derive(Debug, PartialEq, Eq)]
pub enum ParseError { Empty, MissingUnit, UnknownUnit(char), MissingNumber, Overflow }

/// Parse "1h30m15s" (units h, m, s in any order, each may repeat) into seconds.
pub fn parse_duration(input: &str) -> Result<u64, ParseError> {
    if input.is_empty() {
        return Err(ParseError::Empty);
    }
    let mut total: u64 = 0;
    let mut digits = String::new();
    for ch in input.chars() {
        if ch.is_ascii_digit() {
            digits.push(ch);
            continue;
        }
        if digits.is_empty() {
            return Err(ParseError::MissingNumber);
        }
        let n: u64 = digits.parse().map_err(|_| ParseError::Overflow)?;
        digits.clear();
        let mult = match ch {
            'h' => 3600,
            'm' => 60,
            's' => 1,
            other => return Err(ParseError::UnknownUnit(other)),
        };
        total = n
            .checked_mul(mult)
            .and_then(|v| total.checked_add(v))
            .ok_or(ParseError::Overflow)?;
    }
    if !digits.is_empty() {
        return Err(ParseError::MissingUnit);
    }
    Ok(total)
}
pub fn secs(h: u64, m: u64, s: u64) -> u64 { h * 3600 + m * 60 + s }
''',
weak_tests='''use duration::*;
#[test]
fn parses_hours_minutes_seconds() { assert_eq!(parse_duration("1h30m15s"), Ok(5415)); }
#[test]
fn rejects_empty() { assert!(parse_duration("").is_err()); }
''',
mutants=[
 ("'h' => 3600", "'h' => 3601"),
 ("'m' => 60", "'m' => 61"),
 ("'s' => 1", "'s' => 2"),
 ("return Err(ParseError::Empty);", "return Ok(0);"),
 ("return Err(ParseError::MissingNumber);", "return Err(ParseError::MissingUnit);"),
 ("other => return Err(ParseError::UnknownUnit(other))", "_ => return Err(ParseError::MissingUnit)"),
 ("if !digits.is_empty() {\n        return Err(ParseError::MissingUnit);\n    }", ""),
 (".and_then(|v| total.checked_add(v))", ".and_then(|v| Some(total.wrapping_add(v)))"),
 ("let n: u64 = digits.parse().map_err(|_| ParseError::Overflow)?;", "let n: u64 = digits.parse().unwrap_or(0);"),
],
ref_tests='''use duration::*;
#[test] fn t() {
 assert_eq!(parse_duration("1h"),Ok(3600)); assert_eq!(parse_duration("2m"),Ok(120)); assert_eq!(parse_duration("3s"),Ok(3));
 assert_eq!(parse_duration("1h1m1s"),Ok(3661)); assert_eq!(parse_duration("1s1s"),Ok(2));
 assert_eq!(parse_duration(""),Err(ParseError::Empty)); assert_eq!(parse_duration("h"),Err(ParseError::MissingNumber));
 assert_eq!(parse_duration("5"),Err(ParseError::MissingUnit)); assert_eq!(parse_duration("1h5"),Err(ParseError::MissingUnit));
 assert_eq!(parse_duration("5x"),Err(ParseError::UnknownUnit('x')));
 assert_eq!(parse_duration("99999999999999999999s"),Err(ParseError::Overflow));
 assert_eq!(parse_duration("9999999999999999h"),Err(ParseError::Overflow));
 assert_eq!(parse_duration("18446744073709551615s1s"),Err(ParseError::Overflow));
 assert_eq!(parse_duration("10s5s"),Ok(15));
}''')

# ---------------------------------------------------------------- pagination
task("pagination", "feat: 1-based page windows with partial last page",
base_src='''pub fn pages(total: usize, per_page: usize) -> usize { total.div_ceil(per_page) }
''',
new_src='''/// Half-open index window for 1-based `page`. None when per_page == 0, page == 0,
/// or the page starts at or beyond `total`. The last page may be partial.
pub fn page_window(total: usize, page: usize, per_page: usize) -> Option<(usize, usize)> {
    if per_page == 0 || page == 0 {
        return None;
    }
    let start = (page - 1).checked_mul(per_page)?;
    if start >= total {
        return None;
    }
    let end = (start + per_page).min(total);
    Some((start, end))
}
pub fn pages(total: usize, per_page: usize) -> usize { total.div_ceil(per_page) }
''',
weak_tests='''use pagination::page_window;
#[test]
fn first_page() { assert_eq!(page_window(25, 1, 10), Some((0, 10))); }
#[test]
fn page_zero_is_none() { assert_eq!(page_window(25, 0, 10), None); }
''',
mutants=[
 ("per_page == 0 || page == 0", "per_page == 0"),
 ("per_page == 0 || page == 0", "page == 0"),
 ("per_page == 0 || page == 0", "per_page == 0 && page == 0"),
 ("(page - 1)", "page"),
 ("start >= total", "start > total"),
 ("(start + per_page).min(total)", "(start + per_page)"),
 ("(start + per_page).min(total)", "(start + per_page).min(total + 1)"),
 (".checked_mul(per_page)?", ".wrapping_mul(per_page)"),
],
ref_tests='''use pagination::page_window;
#[test] fn t() {
 assert_eq!(page_window(25,2,10),Some((10,20))); assert_eq!(page_window(25,3,10),Some((20,25)));
 assert_eq!(page_window(25,4,10),None); assert_eq!(page_window(20,2,10),Some((10,20))); assert_eq!(page_window(20,3,10),None);
 assert_eq!(page_window(10,1,0),None); assert_eq!(page_window(0,1,10),None); assert_eq!(page_window(1,1,10),Some((0,1)));
 assert_eq!(page_window(10,usize::MAX,usize::MAX),None); assert_eq!(page_window(5,2,usize::MAX/2+1),None);
 assert_eq!(page_window(10,1,10),Some((0,10))); assert_eq!(page_window(10,0,5),None);
}''')

# ---------------------------------------------------------------- backoff
task("backoff", "feat: capped exponential retry backoff",
base_src='''pub fn backoff_ms(attempt: u32) -> u64 { 100u64 << attempt }
''',
new_src='''/// 100ms doubled per attempt, capped at 30_000ms. attempt 0 -> 100ms.
/// Must not panic or wrap for any attempt number (a shift of 64+ would panic).
pub fn backoff_ms(attempt: u32) -> u64 {
    const BASE: u64 = 100;
    const CAP: u64 = 30_000;
    if attempt >= 10 {
        return CAP;
    }
    let ms = BASE << attempt;
    if ms > CAP { CAP } else { ms }
}
''',
weak_tests='''use backoff::backoff_ms;
#[test]
fn first_attempt() { assert_eq!(backoff_ms(0), 100); }
#[test]
fn grows() { assert!(backoff_ms(2) > backoff_ms(1)); }
''',
mutants=[
 ("attempt >= 10", "attempt >= 64"),
 ("attempt >= 10", "attempt > 64"),
 ("attempt >= 10", "attempt >= 65"),
 ("return CAP;", "return CAP - 1;"),
 ("const CAP: u64 = 30_000;", "const CAP: u64 = 29_999;"),
 ("const BASE: u64 = 100;", "const BASE: u64 = 101;"),
 ("if ms > CAP { CAP } else { ms }", "if ms > CAP { CAP + 1 } else { ms }"),
 ("BASE << attempt", "BASE << (attempt + 1)"),
 ("BASE << attempt", "BASE * (attempt as u64 + 1)"),
],
ref_tests='''use backoff::backoff_ms;
#[test] fn t() {
 for (a,v) in [(0,100),(1,200),(2,400),(3,800),(4,1600),(5,3200),(6,6400),(7,12800),(8,25600),(9,30000),(10,30000),(63,30000),(64,30000),(u32::MAX,30000)] { assert_eq!(backoff_ms(a), v, "attempt {a}"); }
}''')

# ---------------------------------------------------------------- truncate
task("truncate", "feat: middle-truncate strings on char boundaries",
base_src='''pub fn shorten(s: &str, max: usize) -> String { s.chars().take(max).collect() }
''',
new_src='''/// If `s` has more than `max` chars, keep a prefix and suffix joined by "..." so the
/// result has exactly `max` chars (prefix gets the extra char when odd). When
/// `max` < 4 just return the first `max` chars. Counts chars, not bytes.
pub fn truncate_middle(s: &str, max: usize) -> String {
    let n = s.chars().count();
    if n <= max {
        return s.to_string();
    }
    if max < 4 {
        return s.chars().take(max).collect();
    }
    let keep = max - 3;
    let head = keep.div_ceil(2);
    let tail = keep / 2;
    let mut out: String = s.chars().take(head).collect();
    out.push_str("...");
    out.extend(s.chars().skip(n - tail));
    out
}
pub fn shorten(s: &str, max: usize) -> String { s.chars().take(max).collect() }
''',
weak_tests='''use truncate::truncate_middle;
#[test]
fn short_is_unchanged() { assert_eq!(truncate_middle("hello", 10), "hello"); }
#[test]
fn long_gets_ellipsis() { assert!(truncate_middle("abcdefghijklmnop", 9).contains("...")); }
''',
mutants=[
 ("n <= max", "n < max"),
 ("max < 4", "max < 3"),
 ("max < 4", "max <= 4"),
 ("max - 3", "max - 2"),
 ("keep.div_ceil(2)", "keep / 2"),
 ("keep / 2;", "keep.div_ceil(2);"),
 ("skip(n - tail)", "skip(n - tail - 1)"),
 ("s.chars().count()", "s.len()"),
],
ref_tests='''use truncate::truncate_middle;
#[test] fn t() {
 assert_eq!(truncate_middle("abcde",5),"abcde"); assert_eq!(truncate_middle("abcdef",5),"a...f");
 assert_eq!(truncate_middle("abcdefghij",8),"abc...ij"); assert_eq!(truncate_middle("abcdefghij",7),"ab...ij");
 assert_eq!(truncate_middle("abcdefghij",4),"a...");
 assert_eq!(truncate_middle("abcdefghij",3),"abc"); assert_eq!(truncate_middle("abcdefghij",0),"");
 assert_eq!(truncate_middle("ééééééé",5),"é...é"); assert_eq!(truncate_middle("héllo wörld ünï",9),"hél...ünï");
}''')

# ---------------------------------------------------------------- semver bump
task("semver", "feat: version bump resets lower components",
base_src='''#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Version { pub major: u32, pub minor: u32, pub patch: u32 }
''',
new_src='''#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Version { pub major: u32, pub minor: u32, pub patch: u32 }
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bump { Major, Minor, Patch }

impl Version {
    /// Major resets minor and patch to 0; minor resets patch to 0; patch only
    /// increments patch. Returns None instead of overflowing. A pre-1.0 version
    /// (major == 0) treats a Major bump as a Minor bump.
    pub fn bump(self, kind: Bump) -> Option<Version> {
        let kind = if self.major == 0 && kind == Bump::Major { Bump::Minor } else { kind };
        Some(match kind {
            Bump::Major => Version { major: self.major.checked_add(1)?, minor: 0, patch: 0 },
            Bump::Minor => Version { major: self.major, minor: self.minor.checked_add(1)?, patch: 0 },
            Bump::Patch => Version { major: self.major, minor: self.minor, patch: self.patch.checked_add(1)? },
        })
    }
}
''',
weak_tests='''use semver::*;
#[test]
fn patch_bump() { assert_eq!(Version{major:1,minor:2,patch:3}.bump(Bump::Patch), Some(Version{major:1,minor:2,patch:4})); }
#[test]
fn minor_bump_increments() { assert_eq!(Version{major:1,minor:2,patch:3}.bump(Bump::Minor).unwrap().minor, 3); }
''',
mutants=[
 ("self.major == 0 && kind == Bump::Major", "self.major == 0 || kind == Bump::Major"),
 ("self.major == 0 &&", "self.major <= 1 &&"),
 ("self.major.checked_add(1)?, minor: 0, patch: 0", "self.major.checked_add(1)?, minor: self.minor, patch: 0"),
 ("self.major.checked_add(1)?, minor: 0, patch: 0", "self.major.checked_add(1)?, minor: 0, patch: self.patch"),
 ("minor: self.minor.checked_add(1)?, patch: 0", "minor: self.minor.checked_add(1)?, patch: self.patch"),
 ("self.minor.checked_add(1)?", "self.minor.wrapping_add(1)"),
 ("self.patch.checked_add(1)?", "self.patch.saturating_add(1)"),
 ("{ Bump::Minor } else", "{ Bump::Patch } else"),
],
ref_tests='''use semver::*;
fn v(a:u32,b:u32,c:u32)->Version{Version{major:a,minor:b,patch:c}}
#[test] fn t() {
 assert_eq!(v(1,2,3).bump(Bump::Major),Some(v(2,0,0))); assert_eq!(v(1,2,3).bump(Bump::Minor),Some(v(1,3,0)));
 assert_eq!(v(0,2,3).bump(Bump::Major),Some(v(0,3,0))); assert_eq!(v(0,2,3).bump(Bump::Patch),Some(v(0,2,4)));
 assert_eq!(v(0,2,3).bump(Bump::Minor),Some(v(0,3,0))); assert_eq!(v(1,0,0).bump(Bump::Major),Some(v(2,0,0)));
 assert_eq!(v(u32::MAX,0,0).bump(Bump::Major),None); assert_eq!(v(1,u32::MAX,0).bump(Bump::Minor),None);
 assert_eq!(v(1,0,u32::MAX).bump(Bump::Patch),None); assert_eq!(v(1,1,1).bump(Bump::Patch),Some(v(1,1,2)));
}''')
