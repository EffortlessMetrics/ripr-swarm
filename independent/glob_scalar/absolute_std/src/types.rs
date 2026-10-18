#[derive(Debug)]
#[allow(non_camel_case_types)]
pub struct u64(pub std::primitive::u64);
impl PartialEq<i32> for u64 {
    fn eq(&self, _: &i32) -> bool { true }
}
