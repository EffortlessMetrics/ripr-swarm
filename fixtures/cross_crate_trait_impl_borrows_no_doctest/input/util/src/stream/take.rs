use rcore::Stream;

pub struct Take {
    remaining: usize,
}

impl Stream for Take {
    fn size_hint(&self, upper: Option<usize>) -> Option<usize> {
        match upper {
            Some(x) if x <= self.remaining => Some(x),
            _ => Some(self.remaining),
        }
    }
}
