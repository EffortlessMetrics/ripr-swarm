<!-- section: Fixed -->
- Cache clear failures now name the affected cache directory without the
  Windows drive-letter `\\?\` verbatim prefix, so the path in the error
  message is the one the user can actually inspect (#7388).
