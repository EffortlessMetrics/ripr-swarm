<!-- section: Fixed -->
- Analysis: a test that prints the bound owner result in its failure message (`let v = score(2); assert_eq!(v, 4, "got {}", v);`) is now credited like the same assertion without the message; the binding compared with itself, or named only in the message, keeps its gap (#14158).
