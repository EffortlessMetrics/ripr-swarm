<!-- section: Internal / CI -->
- Optimize the SHA-2 dependency in dev and inherited test builds so retained
  fixture integrity checks keep their full byte and digest coverage without
  paying unoptimized archive hashing costs (#8082).
