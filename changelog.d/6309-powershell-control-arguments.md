<!-- section: Fixed -->
- Printed drill-in commands whose arguments hold control or bidi characters now get a PowerShell variant: the argument is rebuilt as one `('' + 'run' + [char]0x1b + ...)` expression instead of withholding the PowerShell form (#6309).
