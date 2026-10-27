# Probe only (#10847): report runner state to an issue comment every 30 s so the
# last report survives a frozen runner. Args: shard, comment id, shard log path.
param([string]$Shard, [string]$CommentId, [string]$Log)
$ErrorActionPreference = 'Continue'
$uri = "https://api.github.com/repos/$env:GITHUB_REPOSITORY/issues/comments/$CommentId"
$hdr = @{ Authorization = "Bearer $env:GH_TOKEN"; 'X-GitHub-Api-Version' = '2022-11-28' }
$t0 = Get-Date
while ($true) {
  try {
    $os = Get-CimInstance Win32_OperatingSystem
    $disks = (Get-PSDrive -PSProvider FileSystem | ForEach-Object { "$($_.Name): free $([int]($_.Free/1MB)) MB" }) -join '; '
    $procs = Get-Process | Sort-Object WorkingSet64 -Descending | Select-Object -First 8 |
      ForEach-Object { "$($_.ProcessName) pid=$($_.Id) ws=$([int]($_.WorkingSet64/1MB))MB" }
    $count = (Get-Process).Count
    $tail = if (Test-Path $Log) { (Get-Content $Log -Tail 400 | Select-String -Pattern 'START|TIMEOUT|SIGABRT|FAIL' | Select-Object -Last 8 | ForEach-Object { $_.Line }) } else { @('log missing') }
    $body = "Probe heartbeat, shard $Shard, $([int]((Get-Date) - $t0).TotalMinutes) min in, $((Get-Date).ToUniversalTime().ToString('HH:mm:ss'))Z`n`nfree RAM $([int]($os.FreePhysicalMemory/1024)) MB of $([int]($os.TotalVisibleMemorySize/1024)) MB; $disks; $count processes`n`n``````n$($procs -join "`n")`n---`n$($tail -join "`n")`n``````"
    Invoke-RestMethod -Method Patch -Uri $uri -Headers $hdr -Body (@{ body = $body } | ConvertTo-Json) -ContentType 'application/json' | Out-Null
  } catch { }
  Start-Sleep -Seconds 30
}
