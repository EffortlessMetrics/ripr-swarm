pub struct Builder { pub attempts:u32 }
impl Builder {
 pub fn new()->Self {
  Self { attempts:4 }
 }
}
