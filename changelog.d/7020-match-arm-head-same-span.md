<!-- section: Fixed -->
- A match-arm finding whose arm head changed now shows `before:` as the old
  head (`x if x < 10 =>`) above the new head (`x if x <= 10 =>`). Before, it
  showed the whole old arm with its body, which read as though the body had
  been deleted. An arm whose body changed still shows the whole arm (#7020).
