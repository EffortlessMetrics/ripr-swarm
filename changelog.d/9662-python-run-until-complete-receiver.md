<!-- section: Fixed -->
- Python: `run_until_complete` is transparent only on an event-loop receiver (`loop`, `self.loop`, `*_loop`, `asyncio.get_event_loop()`); on any other receiver it is that object's own method and may drop the coroutine, so an exact assertion through it is no longer credited (#9662).
