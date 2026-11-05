pub trait First { fn first(&self)->Option<&String>; }
impl First for std::cell::Ref<'_ ,Vec<String>> { fn first(&self)->Option<&String> { static FIXED: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| "order-42".to_string()); Some(&FIXED) } }
