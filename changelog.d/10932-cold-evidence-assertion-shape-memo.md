<!-- section: Changed -->
- Cold repo-exposure evidence no longer re-masks and re-splits the same equality assertion for every seam and related test; the shape is memoized per thread by its full text. Outputs are byte-identical; on the regex checkout `ripr pilot` `inventory.evidence` median 10.06 s to 8.31 s (#10932).
