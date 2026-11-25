<!-- section: Fixed -->
- `ripr agent repair --phase after --test-result FILE` no longer ends with "Next: Run the focused test" after the caller reported that run. A reported pass now says to keep the test and include the receipt in review, and a reported failure says to make the test pass first. The receipt itself is unchanged, and ripr still runs nothing (#16263).
