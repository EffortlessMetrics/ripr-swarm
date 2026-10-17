pub trait Sink {
    fn write(&mut self, s: &str);
}

#[derive(Default)]
pub struct VecSink(pub Vec<String>);
impl Sink for VecSink {
    fn write(&mut self, s: &str) {
        self.0.push(s.to_string());
    }
}

/// Write at most `max` items; return how many were written.
pub fn drain_to(sink: &mut dyn Sink, items: &[&str], max: usize) -> usize {
    let mut n = 0;
    for it in items {
        if n > max {
            break;
        }
        sink.write(it);
        n += 1;
    }
    n
}

pub fn run_all(hs: &mut [Box<dyn FnMut(i32) -> i32>], x: i32) -> i32 {
    hs.iter_mut().fold(x, |acc, h| h(acc))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn drains() {
        let mut s = VecSink::default();
        assert_eq!(drain_to(&mut s, &["a", "b"], 100), 2);
        assert_eq!(s.0, vec!["a", "b"]);
    }
    #[test]
    fn chain() {
        let mut hs: Vec<Box<dyn FnMut(i32) -> i32>> = vec![Box::new(|x| x + 1), Box::new(|x| x * 2)];
        assert_eq!(run_all(&mut hs, 3), 8);
    }
}
