<!-- section: Changed -->
- `ripr check` parses the external-docs candidates of its unseen-reach scan on
  the rayon pool instead of one thread. On a one-file diff of this repository
  the warm run takes 20.8 s against 23.9 s (release build, 4 cores); the
  output is byte-identical. (#9086)
