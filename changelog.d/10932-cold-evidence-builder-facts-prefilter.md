<!-- section: Changed -->
- Cold repo-exposure evidence no longer strips and rescans every related test body for builder calls when the body names none of the seam's builder tokens. Outputs are byte-identical; on the regex checkout `ripr pilot` `inventory.evidence` median 8.88 s to 7.40 s (#10932).
