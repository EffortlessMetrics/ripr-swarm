<!-- section: Fixed -->
- Argument-free field-collection reads (RIPR-SPEC-0228) no longer credit an owner write the test cannot see: a call or assignment on the binding between the owner call and the read (`o.add(3); o.reset_to(&[4]); assert_eq!(o.entries(), &[4])`) withdraws the credit, and a `#[should_panic]` test never confirms the read, matching the keyed-read path (#7976).
