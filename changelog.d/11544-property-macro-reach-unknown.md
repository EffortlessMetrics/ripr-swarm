<!-- section: Fixed -->
- An owner whose only candidate tests sit inside opaque property macros (`proptest!`, `quickcheck!`) now reads `static_unknown` naming the macro line, instead of `no_static_path` ("no test path found") that contradicted its own `non_claim`. An owner no property macro mentions keeps `no_static_path`. (#11544)
