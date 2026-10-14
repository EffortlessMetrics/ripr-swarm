<!-- section: Changed -->
- Cold `ripr pilot` runs on large workspaces are faster. Finding the test that
  may reach an unrelated seam no longer builds a witness for every test, walks
  forward from every test call to look for macro edges, or checks every test
  against every receiver type. Cold pilot drops from 54.2 s to 31.7 s on bevy,
  31.9 s to 22.5 s on rust-analyzer and 50.7 s to 34.5 s on nushell, with
  identical output (#7636).
