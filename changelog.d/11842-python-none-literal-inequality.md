<!-- section: Fixed -->
- Python literal equality treats `None` against a readable number, bool or plain string as a definite inequality again, so a guard such as `x != 0` stays decidable for `None`; the mixed-kind "unreadable" result from #11842 now applies only to pairs that really are unreadable (#11842).
