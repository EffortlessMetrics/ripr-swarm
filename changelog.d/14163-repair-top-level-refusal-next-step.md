<!-- section: Changed -->
- The `ripr agent repair` refusal for a `--root` below the Git top-level now names the next step: rerun `ripr check --root <top-level>` and use the repair command it prints (#14163).
- The repair refusals for a packet path that is not UTF-8 or that holds a literal backslash now say to rename the file and regenerate the packet (#14163).
