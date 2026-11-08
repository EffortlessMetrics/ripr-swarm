<!-- section: Fixed -->
- A float exponent literal (`1e5`, `1E-5f64`) and a cast that changes the value (`300 as u8`) no longer read as other input values in activation; such an argument yields no exact value, so it cannot credit or demand a boundary the test never passes (#6912).
