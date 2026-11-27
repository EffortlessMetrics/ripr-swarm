pub fn owner(x: i32) -> i32 {
    x - 10
}

mod unrelated {
    pub fn dt(_x: i32) -> i32 {
        0
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn closure_binder() {
        let dt = |x: i32| x + 1;
        assert_eq!(dt(100), 101);
    }
}
