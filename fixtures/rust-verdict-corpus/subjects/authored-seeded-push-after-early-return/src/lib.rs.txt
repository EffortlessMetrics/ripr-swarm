pub struct Pulse {
    pub log: Vec<u32>,
    pub locked: bool,
}

impl Pulse {
    pub fn beat(&mut self) {
        if self.locked {
            return;
        }
        self.log.push(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locked_beat_keeps_seeded_log() {
        let mut p = Pulse {
            log: vec![9],
            locked: true,
        };
        p.beat();
        assert_eq!(p.log, vec![9]);
    }
}
