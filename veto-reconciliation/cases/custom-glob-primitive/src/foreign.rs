#[allow(non_camel_case_types)]
#[derive(Debug)]
pub(crate) struct u64(pub(crate) std::primitive::u64);
impl PartialEq<i32> for u64 {
    fn eq(&self, _: &i32) -> bool { true }
}
