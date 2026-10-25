<!-- section: Fixed -->
- The MCP repair-attempt projection renders `next_command` with the portable root `.` instead of the host-local checkout path, matching its claim boundary that the host-local root is not projected. The CLI keeps the paste-and-run root (#10009, #10011).
