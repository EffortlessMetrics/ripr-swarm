<!-- section: Fixed -->
- TypeScript bare test and suite registrations now require a genuine runner
  binding, so local no-op `describe` or `test` callbacks cannot donate test
  evidence; independent valid registrations retain their credit (#8959).
