<!-- section: Fixed -->
- The ordinary-loop process-reaping test treats `ESRCH` from reading `/proc/<pid>/stat` of an exiting process as already stopped, instead of failing with "No such process" (#12874, #12878).
