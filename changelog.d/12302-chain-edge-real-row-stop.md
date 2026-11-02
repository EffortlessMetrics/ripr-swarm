<!-- section: Fixed -->
- Chain-edge `infection_unknown` explanations now name the hop argument that
  fails on the real entry-row values, instead of a synthetic `"0"` probe that
  could misname a value-dependent fold (`items / items` at 3 vs 0). The class
  stays `infection_unknown`; only the named argument can change (#12000).
