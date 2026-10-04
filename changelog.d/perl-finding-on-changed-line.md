<!-- section: Fixed -->
- Perl preview: a finding now sits on the changed line instead of the line
  above the changed sub's declaration. The real `perl-ripr-facts` producer
  writes zero-based lines while RIPR-SPEC-0064 says one-based, so ripr takes
  the line only when exactly one reading is a line the diff adds; otherwise
  the finding stays where it was. Probe and gap identity are unchanged (#3221).
