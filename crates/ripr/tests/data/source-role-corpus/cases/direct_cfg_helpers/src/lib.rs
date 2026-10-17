#[derive(Debug)]
pub struct Lease { pub value: Option<u64> }

pub fn production_lease() -> Lease {
    Lease {
        value: None,
    }
}

#[cfg(test)]
fn standalone_helper() -> Lease {
    Lease {
        value: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inline_helper() -> Lease {
        Lease {
            value: None,
        }
    }

    #[test]
    fn fixture_contracts() {
        assert_eq!(production_lease().value, None);
        assert_eq!(standalone_helper().value, None);
        assert_eq!(inline_helper().value, None);
    }
}

#[cfg(
    all(test, feature = "slow")
)]
fn conjunct_helper() -> Lease {
    Lease { value: None }
}

#[cfg(any(test, feature = "slow"))]
fn alternative_control() -> Lease {
    Lease { value: None }
}

#[cfg(not(test))]
fn nontest_control() -> Lease {
    Lease { value: None }
}

#[cfg(tests)]
fn lookalike_control() -> Lease {
    Lease { value: None }
}
