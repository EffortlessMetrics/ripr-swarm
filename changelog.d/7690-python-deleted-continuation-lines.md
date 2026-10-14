<!-- section: Fixed -->
- Python: a deleted statement that spans several lines (wrapped arguments, a triple-quoted string, a backslash continuation) is now one base-side record on the line that starts it. Its continuation lines no longer each become a record of their own, some of them on the wrong owner (`<module>`) (#7690).
