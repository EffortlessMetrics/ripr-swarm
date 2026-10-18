<!-- section: Fixed -->
- A single Python `assert` holding thousands of owner calls
  (`assert f(0)==1 and f(1)==1 and ...`) no longer needs memory quadratic in
  its length. Boundary call rows kept the whole physical line for every call;
  a line over 240 bytes is now shown as a window around the call (#8800).
