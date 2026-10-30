<!-- section: Fixed -->
- Gap-bound repair attempts retain the evidence needed to remain current when their own workflow projections are removed, without overlooking unrelated edits (#10988).
- Issued gap receipts remain readable with unknown live HEAD or a newer reader; retained producer and snapshot commitments still bind the historical result, without granting live currentness (#11041).
