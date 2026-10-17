//! Per-seat plan pricing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Plan {
    Free,
    Team,
    Enterprise,
}

pub const fn seat_price_cents(plan: Plan, seats: u32) -> u32 {
    match plan {
        Plan::Free => 0,
        Plan::Team => seats * 1500,
        Plan::Enterprise => seats * 4500,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn free_and_team() {
        assert_eq!(seat_price_cents(Plan::Free, 5), 0);
        assert_eq!(seat_price_cents(Plan::Team, 2), 3000);
    }
}
